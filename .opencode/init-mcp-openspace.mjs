#!/usr/bin/env node
// init-mcp-openspace.mjs — OpenSpace MCP startup wrapper (pixi-based)
// Uses project pixi.toml environment (Python 3.12.* already configured).
// Auto-installs openspace-mcp on first run.
import { execSync, spawn } from 'node:child_process'
import { existsSync, mkdirSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { platform, env } from 'node:process'

const __dirname = dirname(fileURLToPath(import.meta.url))
const projectRoot = join(__dirname, '..')
const isWin = platform === 'win32'

// 1. Verify pixi is available
// opencode 启动环境 PATH 可能不含 ~/.pixi/bin（pixi 默认安装位置）→ 显式补充
const homeBin = join(env.HOME || env.USERPROFILE || '', '.pixi', 'bin')
if (!process.env.PATH?.includes(homeBin)) {
  process.env.PATH = `${homeBin}:${process.env.PATH || ''}`
}
try {
  execSync('pixi --version', { cwd: projectRoot, stdio: 'pipe' })
} catch {
  console.error('[openspace] pixi not found. Install: curl -fsSL https://pixi.sh/install.sh | bash')
  process.exit(1)
}

// 2. Resolve pixi env bin path and openspace-mcp binary
const pixiEnvBin = join(projectRoot, '.pixi', 'envs', 'default', 'bin')
const pythonBin = isWin
  ? join(pixiEnvBin, 'python.exe')
  : join(pixiEnvBin, 'python')
const binaryName = isWin ? 'openspace-mcp.exe' : 'openspace-mcp'
const binaryPath = join(pixiEnvBin, binaryName)

// 3. Run pixi install if Python not found
if (!existsSync(pythonBin)) {
  console.error('[openspace] pixi env not found, running pixi install...')
  execSync('pixi install', { cwd: projectRoot, stdio: 'inherit' })
}

// 4. Auto-install openspace-mcp if missing
if (!existsSync(binaryPath)) {
  console.error('[openspace] openspace-mcp not found, installing...')
  try {
    execSync('pixi run python -m pip install --quiet "git+https://github.com/hkuds/openspace.git"', {
      cwd: projectRoot, stdio: 'pipe'
    })
    if (!existsSync(binaryPath)) {
      console.error('[openspace] Install completed but binary not found at', binaryPath)
      process.exit(1)
    }
  } catch (e) {
    console.error('[openspace] Failed to install openspace-mcp:', e.message)
    process.exit(1)
  }
}

// 5. Setup workspace and skill dirs
const workspaceDir = join(projectRoot, '.venv-openspace', 'workspace')
if (!existsSync(workspaceDir)) {
  mkdirSync(workspaceDir, { recursive: true })
}

const pathSep = isWin ? ';' : ':'
const skillDirPaths = [
  join(projectRoot, '.agents', 'skills'),
].filter(p => existsSync(p))

console.error('[openspace] Skill dirs:', skillDirPaths.join(', ') || '(none)')

// 6. Validate API key
if (!env.ANTHROPIC_API_KEY && !env.OPENAI_API_KEY) {
  console.error('[openspace] No LLM API key found. Will run in local-only mode.')
}

// 7. Start openspace-mcp (stdio transport)
console.error('[openspace] Starting openspace-mcp...')
const childEnv = {
  ...env,
  OPENSPACE_HOST_SKILL_DIRS: skillDirPaths.join(pathSep),
  OPENSPACE_WORKSPACE: workspaceDir,
}
if (env.ANTHROPIC_API_KEY) childEnv.OPENSPACE_API_KEY = env.ANTHROPIC_API_KEY

const child = spawn(binaryPath, ['--transport', 'stdio'], {
  cwd: projectRoot,
  stdio: 'inherit',
  env: childEnv,
})
child.on('error', (err) => {
  console.error('[openspace] spawn error:', err.message)
  process.exit(1)
})
child.on('exit', (code) => process.exit(code ?? 1))
