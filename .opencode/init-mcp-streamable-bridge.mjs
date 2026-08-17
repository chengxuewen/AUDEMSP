#!/usr/bin/env node
// init-mcp-streamable-bridge.mjs — 通用本地桥: stdio (opencode) ↔ streamable HTTP (MCP v4 服务)
// context7 / grep_app 共用。上游经环境变量 STREAMABLE_MCP_URL 指定。
//
// 背景: opencode 1.18 对 remote MCP 走 SSE(GET)，而 context7 v4 只接受
// streamable HTTP(POST) → 405。本脚本作为 local MCP server 启动，
// 内部用 MCP SDK 1.30 的 StreamableHTTPClientTransport 转发到 context7。
//
// 用法: opencode.json mcp.context7 = { type: "local", command: ["node", ".opencode/init-mcp-context7.mjs"] }

import { createRequire } from 'node:module'
import { existsSync } from 'node:fs'
import { join } from 'node:path'
import { homedir } from 'node:os'

const require = createRequire(import.meta.url)
const candidates = [
  join(homedir(), '.cache/opencode/packages/oh-my-opencode@latest/node_modules/@modelcontextprotocol/sdk'),
  join(homedir(), '.cache/opencode/packages/oh-my-opencode/node_modules/@modelcontextprotocol/sdk'),
]
const sdkRoot = candidates.find(existsSync)
if (!sdkRoot) {
  console.error('[mcp-bridge] MCP SDK not found; run: npm i @modelcontextprotocol/sdk in .opencode/')
  process.exit(1)
}
const sdk = (p) => join(sdkRoot, 'dist/esm', p)
const { StreamableHTTPClientTransport } = await import(sdk('client/streamableHttp.js'))
const { Client } = await import(sdk('client/index.js'))
const { Server } = await import(sdk('server/index.js'))
const { StdioServerTransport } = await import(sdk('server/stdio.js'))
const { ListToolsRequestSchema, CallToolRequestSchema } = await import(sdk('types.js'))

const UPSTREAM = process.env.STREAMABLE_MCP_URL || process.env.CONTEXT7_URL || 'https://mcp.context7.com/mcp'
const API_KEY = process.env.CONTEXT7_API_KEY || ''

async function main() {
  // 1. 连接上游 context7 (streamable HTTP)
  const headers = API_KEY ? { Authorization: `Bearer ${API_KEY}` } : {}
  const client = new Client({ name: 'opencode-context7-bridge', version: '1.0.0' })
  const transport = new StreamableHTTPClientTransport(new URL(UPSTREAM), {
    requestInit: { headers },
  })
  await client.connect(transport)

  // 2. 拿到上游工具列表（透传 inputSchema，不做 Zod 转换）
  const toolsResult = await client.listTools()

  // 3. 暴露为本地 stdio server（底层 handler 直通，避免 Zod schema 校验）
  const server = new Server(
    { name: 'context7-bridge', version: '1.0.0' },
    { capabilities: { tools: {} } },
  )

  server.setRequestHandler(ListToolsRequestSchema, async () => ({
    tools: toolsResult.tools.map((t) => ({
      name: t.name,
      description: t.description ?? '',
      inputSchema: t.inputSchema ?? { type: 'object', properties: {} },
    })),
  }))

  server.setRequestHandler(CallToolRequestSchema, async (req) => {
    const result = await client.callTool({ name: req.params.name, arguments: req.params.arguments })
    return {
      content: Array.isArray(result.content) && result.content.length > 0
        ? result.content
        : [{ type: 'text', text: JSON.stringify(result) }],
      isError: result.isError,
    }
  })

  console.error(`[mcp-bridge] connected, ${toolsResult.tools.length} tools from ${UPSTREAM}`)
  await server.connect(new StdioServerTransport())
}

main().catch((e) => {
  console.error('[mcp-bridge] fatal:', e.message ?? e)
  process.exit(1)
})
