#!/usr/bin/env node
/**
 * Orchestrates gn-drive e2e:
 * 1. Isolated HOME + fixed port
 * 2. Build FE (optionally instrumented) + Go binary
 * 3. Start gn-drive, run Puppeteer specs, coverage gate
 * 4. Teardown
 */
import { spawn, execSync } from 'node:child_process'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const frontendDir = path.resolve(__dirname, '..')
const gnDriveDir = path.resolve(frontendDir, '..')
const coverage = process.argv.includes('--coverage') || process.env.E2E_COVERAGE === '1'
const port = Number(process.env.E2E_PORT || 18765)
const password = process.env.E2E_PASSWORD || 'e2e-test-password'
const baseURL = `http://127.0.0.1:${port}`
const statePath = path.join(frontendDir, 'e2e/.runtime.json')
const nycOut = path.join(frontendDir, '.nyc_output')
const coverageDir = path.join(frontendDir, 'coverage')

let child = null
let homeDir = null
let exitCode = 1

function log(...args) {
  console.log('[e2e]', ...args)
}

function run(cmd, opts = {}) {
  log('run:', cmd)
  execSync(cmd, {
    stdio: 'inherit',
    cwd: opts.cwd || frontendDir,
    env: { ...process.env, ...opts.env },
    shell: true,
  })
}

async function waitForStatus(timeoutMs = 60_000) {
  const start = Date.now()
  while (Date.now() - start < timeoutMs) {
    try {
      const res = await fetch(`${baseURL}/api/v1/status`)
      if (res.ok) {
        const body = await res.json()
        log('server ready:', body)
        return body
      }
    } catch {
      // not up yet
    }
    await new Promise((r) => setTimeout(r, 400))
  }
  throw new Error(`Server did not become ready at ${baseURL} within ${timeoutMs}ms`)
}

function cleanup() {
  if (child && !child.killed) {
    try {
      child.kill('SIGTERM')
    } catch {
      // ignore
    }
    child = null
  }
  if (homeDir && fs.existsSync(homeDir)) {
    try {
      fs.rmSync(homeDir, { recursive: true, force: true })
    } catch {
      // ignore
    }
  }
  try {
    if (fs.existsSync(statePath)) fs.unlinkSync(statePath)
  } catch {
    // ignore
  }
}

process.on('exit', cleanup)
process.on('SIGINT', () => {
  cleanup()
  process.exit(130)
})
process.on('SIGTERM', () => {
  cleanup()
  process.exit(143)
})

async function main() {
  homeDir = fs.mkdtempSync(path.join(os.tmpdir(), 'gn-drive-e2e-'))
  log('HOME=', homeDir)
  log('coverage=', coverage)

  // Clean previous coverage
  if (coverage) {
    fs.rmSync(nycOut, { recursive: true, force: true })
    fs.rmSync(coverageDir, { recursive: true, force: true })
    fs.mkdirSync(nycOut, { recursive: true })
  }

  // Build frontend (instrumented when coverage)
  run('pnpm run build', {
    env: coverage ? { E2E_COVERAGE: '1' } : {},
  })

  // Copy into embed dir
  const distSrc = path.join(frontendDir, 'dist')
  const distDst = path.join(gnDriveDir, 'internal/webui/dist')
  fs.rmSync(distDst, { recursive: true, force: true })
  fs.cpSync(distSrc, distDst, { recursive: true })
  log('embedded dist →', distDst)

  // Build binary
  const binDir = path.join(gnDriveDir, 'bin')
  fs.mkdirSync(binDir, { recursive: true })
  const binPath = path.join(binDir, 'gn-drive-e2e')
  run(`go build -o "${binPath}" ./cmd/gn-drive`, { cwd: gnDriveDir })

  // Start server
  log('starting', binPath, `--port ${port}`)
  child = spawn(binPath, ['run', '--port', String(port), '--no-browser'], {
    env: {
      ...process.env,
      HOME: homeDir,
      // Linux isolation; macOS still uses HOME/.config/gn-drive
      XDG_CONFIG_HOME: path.join(homeDir, '.config'),
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  })
  child.stdout?.on('data', (d) => process.stdout.write(`[gn-drive] ${d}`))
  child.stderr?.on('data', (d) => process.stderr.write(`[gn-drive] ${d}`))
  child.on('exit', (code, signal) => {
    log(`gn-drive exited code=${code} signal=${signal}`)
  })

  const status = await waitForStatus()

  // Ensure master password is configured and app is locked so e2e can exercise
  // the unlock gate. Fresh HOME starts open (setup=false, unlocked=true).
  if (!status.setup) {
    log('pre-setup master password via API')
    const setupRes = await fetch(`${baseURL}/api/v1/auth/setup`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ password }),
    })
    if (!setupRes.ok) {
      throw new Error(`auth setup failed: ${setupRes.status} ${await setupRes.text()}`)
    }
  }
  // Always lock so suites start from a known locked state.
  {
    const lockRes = await fetch(`${baseURL}/api/v1/auth/lock`, { method: 'POST' })
    if (!lockRes.ok) {
      log('warn: lock failed', lockRes.status, await lockRes.text())
    } else {
      log('app locked for e2e')
    }
  }

  fs.writeFileSync(
    statePath,
    JSON.stringify(
      {
        baseURL,
        password,
        port,
        homeDir,
        coverage,
      },
      null,
      2,
    ),
  )

  // Run tests with node:test via tsx
  try {
    // Serial: suites share one server and lock/unlock auth state.
    run('pnpm exec tsx --test --test-concurrency=1 e2e/specs/**/*.spec.ts', {
      env: {
        E2E_COVERAGE: coverage ? '1' : '0',
      },
    })
    exitCode = 0
  } catch {
    exitCode = 1
  }

  // Coverage report + gate (gate runs even if a soft-assert test failed, so we
  // always surface the percentage; only fail on gate when tests passed.)
  if (coverage) {
    try {
      run('pnpm exec nyc report', { cwd: frontendDir })
      if (exitCode === 0) {
        run('pnpm exec nyc check-coverage', { cwd: frontendDir })
      }
    } catch {
      exitCode = 1
    }
  }

  if (child && !child.killed) {
    child.kill('SIGTERM')
    await new Promise((r) => setTimeout(r, 500))
    try {
      child.kill('SIGKILL')
    } catch {
      // ignore
    }
  }
  child = null

  process.exit(exitCode)
}

main().catch((err) => {
  console.error('[e2e] fatal:', err)
  cleanup()
  process.exit(1)
})
