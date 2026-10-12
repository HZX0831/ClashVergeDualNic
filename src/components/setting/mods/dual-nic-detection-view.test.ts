import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'
import type { DualNicDetection } from '@/services/dual-nic'
import { DualNicDetectionView } from './dual-nic-detection-view'

const result: DualNicDetection = {
  configured: true,
  results: [['代理连接', true, '60 ms']],
  egress: [
    {
      label: '代理连接',
      interface: 'USB 网卡',
      source_addresses: [],
      status: 'notObserved',
      detail: 'UDP requires capture',
    },
  ],
}
it('does not present connectivity alone as verified egress', () => {
  const html = renderToStaticMarkup(
    createElement(DualNicDetectionView, { result }),
  )
  expect(html).toContain('运行配置符合双网卡策略')
  expect(html).toContain('尚未取得出口证据')
  expect(html).not.toContain('已核对物理源地址')
})
it('shows measured source addresses and configuration errors independently', () => {
  const html = renderToStaticMarkup(
    createElement(DualNicDetectionView, {
      result: {
        ...result,
        configured: false,
        configuration_error: 'policy changed',
        egress: [
          {
            ...result.egress[0]!,
            status: 'verified',
            source_addresses: ['192.168.1.2'],
          },
        ],
      },
    }),
  )
  expect(html).toContain('运行配置校验失败')
  expect(html).toContain('policy changed')
  expect(html).toContain('已核对物理源地址')
  expect(html).toContain('192.168.1.2')
})
