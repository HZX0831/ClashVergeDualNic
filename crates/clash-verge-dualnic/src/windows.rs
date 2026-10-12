use crate::{Adapter, AdapterCounters};
use anyhow::{Result, bail};
use std::{
    ffi::c_void,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};
use windows_sys::Win32::{
    Foundation::{ERROR_BUFFER_OVERFLOW, HANDLE, INVALID_HANDLE_VALUE},
    NetworkManagement::{IpHelper::*, Ndis::IfOperStatusUp},
    Networking::WinSock::{AF_INET, AF_INET6, AF_UNSPEC, SOCKET_ADDRESS},
    System::Diagnostics::ToolHelp::*,
};

unsafe fn wide(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut len = 0;
    unsafe {
        while *p.add(len) != 0 {
            len += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
    }
}
unsafe fn ip(a: &SOCKET_ADDRESS) -> Option<IpAddr> {
    if a.lpSockaddr.is_null() {
        return None;
    }
    unsafe {
        match (*a.lpSockaddr).sa_family {
            AF_INET if a.iSockaddrLength >= 16 => {
                let b = std::slice::from_raw_parts(a.lpSockaddr.cast::<u8>().add(4), 4);
                Some(Ipv4Addr::new(b[0], b[1], b[2], b[3]).into())
            }
            AF_INET6 if a.iSockaddrLength >= 28 => {
                let b = std::slice::from_raw_parts(a.lpSockaddr.cast::<u8>().add(8), 16);
                let mut octets = [0; 16];
                octets.copy_from_slice(b);
                Some(Ipv6Addr::from(octets).into())
            }
            _ => None,
        }
    }
}

pub fn adapters() -> Result<Vec<Adapter>> {
    inventory(true)
}

fn inventory(physical_only: bool) -> Result<Vec<Adapter>> {
    let mut size = 16384_u32;
    for _ in 0..3 {
        // IP Helper returns a linked structure with pointers into this aligned allocation.
        let mut storage = vec![0_u64; (size as usize).div_ceil(8)];
        let first = storage.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
        let status = unsafe {
            GetAdaptersAddresses(
                AF_UNSPEC as u32,
                GAA_FLAG_INCLUDE_GATEWAYS | GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER,
                std::ptr::null(),
                first,
                &mut size,
            )
        };
        if status == ERROR_BUFFER_OVERFLOW {
            continue;
        }
        if status != 0 {
            bail!("GetAdaptersAddresses failed: {status}");
        }
        let mut result = Vec::new();
        let mut p = first;
        while !p.is_null() {
            unsafe {
                let a = &*p;
                let mut row = MIB_IF_ROW2::default();
                row.InterfaceLuid = a.Luid;
                if GetIfEntry2(&mut row) == 0
                    && (!physical_only
                        || (row.InterfaceAndOperStatusFlags._bitfield & 1 != 0 && (a.IfType == 6 || a.IfType == 71)))
                {
                    let g = row.InterfaceGuid;
                    let guid = format!(
                        "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
                        g.data1,
                        g.data2,
                        g.data3,
                        g.data4[0],
                        g.data4[1],
                        g.data4[2],
                        g.data4[3],
                        g.data4[4],
                        g.data4[5],
                        g.data4[6],
                        g.data4[7]
                    );
                    let mut addresses = Vec::new();
                    let mut u = a.FirstUnicastAddress;
                    while !u.is_null() {
                        if let Some(v) = ip(&(*u).Address) {
                            addresses.push(v);
                        }
                        u = (*u).Next;
                    }
                    let mut gateways = Vec::new();
                    let mut gw = a.FirstGatewayAddress;
                    while !gw.is_null() {
                        if let Some(v) = ip(&(*gw).Address) {
                            if !v.is_unspecified() {
                                gateways.push(v);
                            }
                        }
                        gw = (*gw).Next;
                    }
                    let ipv4 = addresses.iter().any(
                        |ip| matches!(ip,IpAddr::V4(v) if !v.is_link_local()&&!v.is_loopback()&&!v.is_unspecified()),
                    ) && gateways.iter().any(IpAddr::is_ipv4);
                    let ipv6=addresses.iter().any(|ip|matches!(ip,IpAddr::V6(v) if !v.is_unicast_link_local()&&!v.is_loopback()&&!v.is_unspecified()))&&gateways.iter().any(IpAddr::is_ipv6);
                    result.push(Adapter {
                        guid,
                        name: wide(a.FriendlyName),
                        description: wide(a.Description),
                        up: a.OperStatus == IfOperStatusUp,
                        addresses: addresses.iter().map(ToString::to_string).collect(),
                        gateways: gateways.iter().map(ToString::to_string).collect(),
                        ipv4,
                        ipv6,
                    });
                }
                p = a.Next;
            }
        }
        result.sort_by(|a, b| a.name.cmp(&b.name));
        return Ok(result);
    }
    bail!("Adapter inventory changed repeatedly during enumeration")
}

fn guid_string(g: windows_sys::core::GUID) -> String {
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        g.data1,
        g.data2,
        g.data3,
        g.data4[0],
        g.data4[1],
        g.data4[2],
        g.data4[3],
        g.data4[4],
        g.data4[5],
        g.data4[6],
        g.data4[7]
    )
}
pub fn network_counters() -> Result<Vec<AdapterCounters>> {
    let inventory = inventory(false)?;
    let mut table = std::ptr::null_mut();
    let status = unsafe { GetIfTable2(&mut table) };
    anyhow::ensure!(status == 0 && !table.is_null(), "GetIfTable2 failed: {status}");
    let mut result = Vec::new();
    unsafe {
        let rows = std::slice::from_raw_parts((*table).Table.as_ptr(), (*table).NumEntries as usize);
        for row in rows {
            if let Some(adapter) = inventory
                .iter()
                .find(|a| a.guid.eq_ignore_ascii_case(&guid_string(row.InterfaceGuid)))
            {
                result.push(AdapterCounters {
                    adapter: adapter.clone(),
                    physical: row.InterfaceAndOperStatusFlags._bitfield & 1 != 0 && (row.Type == 6 || row.Type == 71),
                    interface_id: row.InterfaceIndex.to_string(),
                    received: row.InOctets.to_string(),
                    sent: row.OutOctets.to_string(),
                });
            }
        }
        FreeMibTable(table.cast());
    }
    result.sort_by(|a, b| a.adapter.name.cmp(&b.adapter.name));
    Ok(result)
}

pub fn official_client_running() -> Result<bool> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        bail!("Unable to inspect client ownership");
    }
    let mut entry = PROCESSENTRY32W::default();
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut found = false;
    unsafe {
        let mut ok = Process32FirstW(snapshot, &mut entry);
        while ok != 0 {
            let name = wide(entry.szExeFile.as_ptr()).to_lowercase();
            if name == "clash-verge.exe" || name == "clash-verge-dev.exe" {
                found = true;
                break;
            }
            ok = Process32NextW(snapshot, &mut entry);
        }
        windows_sys::Win32::Foundation::CloseHandle(snapshot);
    }
    Ok(found)
}

pub fn pipe_present(name: &str) -> bool {
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        if windows_sys::Win32::System::Pipes::WaitNamedPipeW(wide.as_ptr(), 1) != 0 {
            return true;
        }
        matches!(windows_sys::Win32::Foundation::GetLastError(), 5 | 121 | 231)
    }
}

struct Notifier {
    callback: Box<dyn Fn() + Send + Sync>,
}
pub struct Notifications {
    handles: Vec<usize>,
    _context: Box<Notifier>,
}
impl Drop for Notifications {
    fn drop(&mut self) {
        for handle in &self.handles {
            unsafe {
                CancelMibChangeNotify2(*handle as HANDLE);
            }
        }
    }
}
unsafe fn notify(context: *const c_void) {
    if !context.is_null() {
        unsafe {
            ((*context.cast::<Notifier>()).callback)();
        }
    }
}
unsafe extern "system" fn interface_changed(
    ctx: *const c_void,
    _row: *const MIB_IPINTERFACE_ROW,
    _kind: MIB_NOTIFICATION_TYPE,
) {
    unsafe {
        notify(ctx);
    }
}
unsafe extern "system" fn address_changed(
    ctx: *const c_void,
    _row: *const MIB_UNICASTIPADDRESS_ROW,
    _kind: MIB_NOTIFICATION_TYPE,
) {
    unsafe {
        notify(ctx);
    }
}
unsafe extern "system" fn route_changed(
    ctx: *const c_void,
    _row: *const MIB_IPFORWARD_ROW2,
    _kind: MIB_NOTIFICATION_TYPE,
) {
    unsafe {
        notify(ctx);
    }
}
pub fn watch(callback: impl Fn() + Send + Sync + 'static) -> Result<Notifications> {
    let mut n = Notifications {
        handles: Vec::new(),
        _context: Box::new(Notifier {
            callback: Box::new(callback),
        }),
    };
    let context = (&*n._context as *const Notifier).cast::<c_void>();
    let mut h = std::ptr::null_mut();
    let status = unsafe { NotifyIpInterfaceChange(AF_UNSPEC, Some(interface_changed), context, false, &mut h) };
    if status != 0 {
        bail!("Interface notification failed: {status}");
    }
    n.handles.push(h as usize);
    let status = unsafe { NotifyUnicastIpAddressChange(AF_UNSPEC, Some(address_changed), context, false, &mut h) };
    if status != 0 {
        bail!("Address notification failed: {status}");
    }
    n.handles.push(h as usize);
    let status = unsafe { NotifyRouteChange2(AF_UNSPEC, Some(route_changed), context, false, &mut h) };
    if status != 0 {
        bail!("Route notification failed: {status}");
    }
    n.handles.push(h as usize);
    Ok(n)
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TcpSocket {
    pub local_ip: IpAddr,
    pub remote_ip: IpAddr,
    pub remote_port: u16,
    pub local_port: u16,
}

pub fn tcp_sockets(pid: u32) -> Result<Vec<TcpSocket>> {
    let mut sockets = Vec::new();
    for family in [AF_INET, AF_INET6] {
        let mut size = 0_u32;
        unsafe {
            GetExtendedTcpTable(
                std::ptr::null_mut(),
                &mut size,
                0,
                family as u32,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            );
        }
        for attempt in 0..3 {
            if size < 4 {
                break;
            }
            let mut storage = vec![0_u64; (size as usize).div_ceil(8)];
            let status = unsafe {
                GetExtendedTcpTable(
                    storage.as_mut_ptr().cast(),
                    &mut size,
                    0,
                    family as u32,
                    TCP_TABLE_OWNER_PID_ALL,
                    0,
                )
            };
            if status == 122 && attempt < 2 {
                continue;
            }
            if status != 0 {
                bail!("GetExtendedTcpTable failed: {status}");
            }
            unsafe {
                if family == AF_INET {
                    let table = &*storage.as_ptr().cast::<MIB_TCPTABLE_OWNER_PID>();
                    anyhow::ensure!(
                        4 + table.dwNumEntries as usize * std::mem::size_of::<MIB_TCPROW_OWNER_PID>()
                            <= storage.len() * 8,
                        "Invalid IPv4 TCP table"
                    );
                    for row in std::slice::from_raw_parts(table.table.as_ptr(), table.dwNumEntries as usize) {
                        if row.dwOwningPid == pid && row.dwState == 5 {
                            sockets.push(TcpSocket {
                                local_ip: Ipv4Addr::from(row.dwLocalAddr.to_ne_bytes()).into(),
                                remote_ip: Ipv4Addr::from(row.dwRemoteAddr.to_ne_bytes()).into(),
                                remote_port: u16::from_be(row.dwRemotePort as u16),
                                local_port: u16::from_be(row.dwLocalPort as u16),
                            });
                        }
                    }
                } else {
                    let table = &*storage.as_ptr().cast::<MIB_TCP6TABLE_OWNER_PID>();
                    anyhow::ensure!(
                        4 + table.dwNumEntries as usize * std::mem::size_of::<MIB_TCP6ROW_OWNER_PID>()
                            <= storage.len() * 8,
                        "Invalid IPv6 TCP table"
                    );
                    for row in std::slice::from_raw_parts(table.table.as_ptr(), table.dwNumEntries as usize) {
                        if row.dwOwningPid == pid && row.dwState == 5 {
                            sockets.push(TcpSocket {
                                local_ip: Ipv6Addr::from(row.ucLocalAddr).into(),
                                remote_ip: Ipv6Addr::from(row.ucRemoteAddr).into(),
                                remote_port: u16::from_be(row.dwRemotePort as u16),
                                local_port: u16::from_be(row.dwLocalPort as u16),
                            });
                        }
                    }
                }
            }
            break;
        }
    }
    Ok(sockets)
}

#[cfg(test)]
mod egress_tests {
    #[test]
    fn reads_an_owned_live_socket_and_decodes_its_remote_port() {
        use std::net::{TcpListener, TcpStream};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let _client = TcpStream::connect(address).unwrap();
        let (_server, _) = listener.accept().unwrap();
        let sockets = super::tcp_sockets(std::process::id()).unwrap();
        assert!(sockets.iter().any(|socket| socket.local_ip.is_loopback()
            && socket.remote_ip == address.ip()
            && socket.remote_port == address.port()));
    }
}

#[cfg(test)]
mod counter_tests {
    #[test]
    fn reads_real_windows_interface_counters_as_decimal_strings() {
        let counters = super::network_counters().unwrap();
        assert!(!counters.is_empty());
        let physical = super::adapters().unwrap();
        for adapter in physical {
            assert!(counters.iter().any(|c| c.adapter.guid == adapter.guid && c.physical));
        }
        for counter in counters {
            assert!(counter.received.parse::<u64>().is_ok() && counter.sent.parse::<u64>().is_ok());
        }
    }
}
