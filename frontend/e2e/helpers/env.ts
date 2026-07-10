import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const statePath = path.resolve(__dirname, '../.runtime.json')

export interface E2ERuntime {
  baseURL: string
  password: string
  port: number
  homeDir: string
  coverage: boolean
}

export function loadRuntime(): E2ERuntime {
  if (!fs.existsSync(statePath)) {
    throw new Error(
      `Missing ${statePath}. Run via pnpm run test:e2e (scripts/e2e-run.mjs starts the server).`,
    )
  }
  return JSON.parse(fs.readFileSync(statePath, 'utf8')) as E2ERuntime
}

export function stateFilePath(): string {
  return statePath
}
