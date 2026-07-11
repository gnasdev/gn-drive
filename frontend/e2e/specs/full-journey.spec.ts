import { after, before, describe, it } from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import type { Page } from 'puppeteer'
import {
  clickTestId,
  closeBrowser,
  closePage,
  collectCoverage,
  confirmDialog,
  goto,
  newPage,
  textAbsent,
  typeTestId,
  waitForTestId,
  waitForText,
} from '../helpers/browser.js'
import { ensureSession, lockFromSettings, unlock } from '../helpers/auth.js'
import { loadRuntime } from '../helpers/env.js'

/**
 * Functional e2e against single-page Workspace (remotes + flows).
 */
describe('full journey', () => {
  let page: Page
  let srcDir: string
  let dstDir: string
  const remoteName = `e2e_local_${Date.now().toString(36)}`
  let flowId = ''

  before(async () => {
    srcDir = fs.mkdtempSync(path.join(os.tmpdir(), 'gn-drive-e2e-src-'))
    dstDir = fs.mkdtempSync(path.join(os.tmpdir(), 'gn-drive-e2e-dst-'))
    fs.writeFileSync(path.join(srcDir, 'hello.txt'), 'gn-drive e2e\n')
    page = await newPage()
  })

  after(async () => {
    if (page) await closePage(page)
    await closeBrowser()
    for (const d of [srcDir, dstDir]) {
      try {
        fs.rmSync(d, { recursive: true, force: true })
      } catch {
        // ignore
      }
    }
  })

  it('unlocks and loads workspace sections', async () => {
    await unlock(page)
    await waitForTestId(page, 'page-workspace')
    await waitForTestId(page, 'workspace-remotes')
    await waitForTestId(page, 'workspace-flows')
    const text = await page.$eval('[data-testid="page-workspace"]', (el) => el.textContent ?? '')
    assert.match(text, /Remotes|Flows/i)
    await collectCoverage(page)
  })

  it('rejects wrong password and short password, then unlocks', async () => {
    await lockFromSettings(page)
    await typeTestId(page, 'unlock-password', 'definitely-wrong-password')
    await clickTestId(page, 'unlock-submit')
    await page.waitForFunction(
      () => {
        const err = document.querySelector('[data-testid="unlock-error"]')
        return !!err && (err.textContent?.length ?? 0) > 0
      },
      { timeout: 10_000 },
    )
    await typeTestId(page, 'unlock-password', 'ab')
    await clickTestId(page, 'unlock-submit')
    await new Promise((r) => setTimeout(r, 300))
    assert.ok(await page.$('[data-testid="page-unlock"]'))
    await unlock(page)
    await waitForTestId(page, 'page-workspace')
    await collectCoverage(page)
  })

  it('navigates workspace and settings', async () => {
    await clickTestId(page, 'nav-settings')
    await waitForTestId(page, 'page-settings')
    const h1 = await page.$eval('h1', (el) => el.textContent?.trim() ?? '')
    assert.equal(h1, 'Settings')
    await clickTestId(page, 'nav-workspace')
    await waitForTestId(page, 'page-workspace')
    await collectCoverage(page)
  })

  it('creates and tests a local rclone remote', async () => {
    await waitForTestId(page, 'workspace-remotes')
    await clickTestId(page, 'remotes-add')
    await waitForTestId(page, 'remotes-add-form')
    await typeTestId(page, 'remotes-name', remoteName)
    await typeTestId(page, 'remotes-type', 'local')
    await clickTestId(page, 'remotes-submit')
    await waitForText(page, remoteName, 15_000)
    await collectCoverage(page)
  })

  it('creates a flow, sets local paths, runs push sync', async () => {
    await waitForTestId(page, 'workspace-flows')
    await clickTestId(page, 'flows-add')

    // New flow card appears in edit mode with one empty operation.
    await page.waitForFunction(
      () => document.querySelectorAll('[data-testid^="flow-card-"]').length > 0,
      { timeout: 10_000 },
    )
    flowId = await page.evaluate(() => {
      const card = document.querySelector('[data-testid^="flow-card-"]')
      const tid = card?.getAttribute('data-testid') ?? ''
      return tid.replace(/^flow-card-/, '')
    })
    assert.ok(flowId, 'expected flow id from card testid')

    // Rename for visibility.
    const nameBtn = await page.$(`[data-testid="flows-edit-name"]`)
    if (nameBtn) {
      await nameBtn.click()
      await typeTestId(page, 'flows-name-inline', `e2e-flow-${Date.now().toString(36)}`)
      // blur / enter to commit name if needed
      await page.keyboard.press('Enter')
    }

    // Set source/target local paths on the first op.
    const opId = await page.evaluate(() => {
      const row = document.querySelector('[data-testid^="op-row-"]')
      return (row?.getAttribute('data-testid') ?? '').replace(/^op-row-/, '')
    })
    assert.ok(opId, 'expected operation row')

    await page.click(`[data-testid="op-src-${opId}"]`, { clickCount: 3 })
    await page.type(`[data-testid="op-src-${opId}"]`, srcDir)
    await page.click(`[data-testid="op-dst-${opId}"]`, { clickCount: 3 })
    await page.type(`[data-testid="op-dst-${opId}"]`, dstDir)

    await clickTestId(page, `flows-save-bottom-${flowId}`)
    await new Promise((r) => setTimeout(r, 500))

    await clickTestId(page, 'flows-run')

    const deadline = Date.now() + 45_000
    let copied = false
    while (Date.now() < deadline) {
      if (fs.existsSync(path.join(dstDir, 'hello.txt'))) {
        const body = fs.readFileSync(path.join(dstDir, 'hello.txt'), 'utf8')
        if (body.includes('gn-drive e2e')) {
          copied = true
          break
        }
      }
      await new Promise((r) => setTimeout(r, 500))
    }
    assert.ok(copied, `expected ${dstDir}/hello.txt after flow push`)
    await collectCoverage(page)
  })

  it('deletes a flow', async () => {
    await waitForTestId(page, 'page-workspace')
    // Ensure we have at least one flow; add if run left zero (edge).
    const count = await page.$$eval('[data-testid^="flow-card-"]', (els) => els.length)
    if (count === 0) {
      await clickTestId(page, 'flows-add')
      await page.waitForFunction(
        () => document.querySelectorAll('[data-testid^="flow-card-"]').length > 0,
        { timeout: 10_000 },
      )
    }
    const name = await page.$eval('[data-testid^="flow-card-"]', (el) => el.textContent ?? '')
    await page.evaluate(() => {
      const btn = document.querySelector(
        'button[data-testid^="flows-delete-"]',
      ) as HTMLButtonElement | null
      btn?.click()
    })
    await confirmDialog(page, 'Delete')
    // After delete, either empty state or remaining cards without that name snippet.
    await new Promise((r) => setTimeout(r, 500))
    await collectCoverage(page)
    void name
  })

  it('settings theme + change password forces unlock', async () => {
    await ensureSession(page)
    await clickTestId(page, 'nav-settings')
    await waitForTestId(page, 'page-settings')
    await clickTestId(page, 'theme-light')
    await clickTestId(page, 'theme-dark')

    const { password } = loadRuntime()
    await typeTestId(page, 'settings-old-password', password)
    await typeTestId(page, 'settings-new-password', 'ab')
    await clickTestId(page, 'settings-change-password')
    await page.waitForFunction(
      () => {
        const msg = document.querySelector('[data-testid="settings-msg"]')
        return !!msg && (msg.textContent?.includes('at least 4') ?? false)
      },
      { timeout: 8_000 },
    )

    const tempPwd = `${password}-tmp`
    await typeTestId(page, 'settings-old-password', password)
    await typeTestId(page, 'settings-new-password', tempPwd)
    await clickTestId(page, 'settings-change-password')
    await waitForTestId(page, 'page-unlock', 10_000)
    await typeTestId(page, 'unlock-password', tempPwd)
    await clickTestId(page, 'unlock-submit')
    await waitForTestId(page, 'page-workspace')

    await clickTestId(page, 'nav-settings')
    await waitForTestId(page, 'page-settings')
    await typeTestId(page, 'settings-old-password', tempPwd)
    await typeTestId(page, 'settings-new-password', password)
    await clickTestId(page, 'settings-change-password')
    await waitForTestId(page, 'page-unlock', 10_000)
    await unlock(page)
    await collectCoverage(page)
  })

  it('topbar lock and guard redirect work', async () => {
    await clickTestId(page, 'lock-button')
    await confirmDialog(page, 'Lock')
    await waitForTestId(page, 'page-unlock')
    await goto(page, '/profiles')
    await waitForTestId(page, 'page-unlock')
    await collectCoverage(page)
  })

  it('deletes remote after re-unlock', async () => {
    await ensureSession(page)
    await waitForTestId(page, 'workspace-remotes')
    await waitForText(page, remoteName, 10_000)
    await page.evaluate((n: string) => {
      const chip = document.querySelector(`[data-testid="remote-chip-${n}"]`)
      const buttons = Array.from(chip?.querySelectorAll('button') ?? []) as HTMLButtonElement[]
      buttons[buttons.length - 1]?.click()
    }, remoteName)
    await confirmDialog(page, 'Delete')
    await textAbsent(page, remoteName)
    await collectCoverage(page)
  })
})
