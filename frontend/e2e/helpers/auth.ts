import type { Page } from 'puppeteer'
import {
  clickTestId,
  collectCoverage,
  confirmDialog,
  goto,
  typeTestId,
  waitForTestId,
} from './browser.js'
import { loadRuntime } from './env.js'

/**
 * Ensure this browser context has a valid session cookie.
 *
 * Backend "unlocked" is process-wide, but the session cookie is per browser
 * context. A prior test may have unlocked the process without giving this
 * context a cookie, so we always call /auth/unlock (idempotent when already
 * unlocked) to mint a cookie before interacting with protected APIs.
 */
export async function ensureSession(page: Page, password?: string): Promise<void> {
  const pwd = password ?? loadRuntime().password
  await goto(page, '/')
  const result = await page.evaluate(async (p: string) => {
    const r = await fetch('/api/v1/auth/unlock', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      credentials: 'same-origin',
      body: JSON.stringify({ password: p }),
    })
    const text = await r.text()
    return { ok: r.ok, status: r.status, text }
  }, pwd)
  if (!result.ok) {
    throw new Error(`auth unlock failed: ${result.status} ${result.text}`)
  }
  // Reload so Vue router/auth store pick up unlocked session (cookie alone is not enough).
  await goto(page, '/')
  await waitForTestId(page, 'page-dashboard')
}

export async function unlock(page: Page, password?: string): Promise<void> {
  const pwd = password ?? loadRuntime().password
  await goto(page, '/unlock')
  const unlockForm = await page.$('[data-testid="page-unlock"]')
  if (unlockForm) {
    await typeTestId(page, 'unlock-password', pwd)
    const hasConfirm = await page.$('[data-testid="unlock-confirm"]')
    if (hasConfirm) {
      await typeTestId(page, 'unlock-confirm', pwd)
    }
    await clickTestId(page, 'unlock-submit')
    await waitForTestId(page, 'page-dashboard')
    return
  }
  // Process already unlocked — still need a cookie for this context.
  await ensureSession(page, pwd)
  await goto(page, '/')
  await waitForTestId(page, 'page-dashboard')
}

export async function ensureUnlocked(page: Page): Promise<void> {
  await ensureSession(page)
  await goto(page, '/')
  // If somehow still on unlock (e.g. race), use UI.
  const unlockForm = await page.$('[data-testid="page-unlock"]')
  if (unlockForm) {
    await unlock(page)
  } else {
    await waitForTestId(page, 'page-dashboard')
  }
  await collectCoverage(page)
}

export async function lockFromSettings(page: Page): Promise<void> {
  await clickTestId(page, 'nav-settings')
  await waitForTestId(page, 'page-settings')
  await clickTestId(page, 'settings-lock')
  await waitForTestId(page, 'page-unlock')
}

export async function lockFromTopbar(page: Page): Promise<void> {
  await clickTestId(page, 'lock-button')
  await confirmDialog(page, 'Lock')
  await waitForTestId(page, 'page-unlock')
}
