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
 * Functional e2e against single-page Workspace (desktop v0.4-style shell).
 */
describe('full journey', () => {
  let page: Page
  let srcDir: string
  let dstDir: string
  const profileName = `e2e-sync-${Date.now().toString(36)}`
  const remoteName = `e2e_local_${Date.now().toString(36)}`

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
    await waitForTestId(page, 'workspace-operations')
    await waitForTestId(page, 'workspace-boards')
    const text = await page.$eval('[data-testid="page-workspace"]', (el) => el.textContent ?? '')
    assert.match(text, /Operations|Remotes|Boards/i)
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

  it('creates a profile (operation) with absolute local paths', async () => {
    await waitForTestId(page, 'page-workspace')
    await clickTestId(page, 'profiles-add')
    await waitForTestId(page, 'profiles-add-form')
    await typeTestId(page, 'profiles-name', profileName)
    await typeTestId(page, 'profiles-from', srcDir)
    await typeTestId(page, 'profiles-to', dstDir)
    await typeTestId(page, 'profiles-direction', 'push')
    await clickTestId(page, 'profiles-submit')
    await waitForText(page, profileName)
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

  it('operations: push sync copies file to destination', async () => {
    await waitForTestId(page, 'workspace-operations')
    // Run the profile we created — click Run on that row.
    await page.evaluate((n: string) => {
      const row = document.querySelector(`[data-testid="profile-row-${n}"]`)
      const btn = row?.querySelector('button') as HTMLButtonElement | null
      // first primary-ish button is Run
      const buttons = Array.from(row?.querySelectorAll('button') ?? []) as HTMLButtonElement[]
      const run = buttons.find((b) => /Run|Chạy/i.test(b.textContent ?? ''))
      run?.click()
    }, profileName)

    const deadline = Date.now() + 30_000
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
    assert.ok(copied, `expected ${dstDir}/hello.txt after push sync`)
    await collectCoverage(page)
  })

  it('creates board with edge, executes DAG, deletes', async () => {
    const name = `e2e-board-${Date.now()}`
    const boardSrc = fs.mkdtempSync(path.join(os.tmpdir(), 'gn-drive-board-src-'))
    const boardDst = fs.mkdtempSync(path.join(os.tmpdir(), 'gn-drive-board-dst-'))
    fs.writeFileSync(path.join(boardSrc, 'board.txt'), 'from-board\n')

    await waitForTestId(page, 'workspace-boards')
    await clickTestId(page, 'boards-add')
    await waitForTestId(page, 'boards-add-form')
    await typeTestId(page, 'boards-name', name)
    await typeTestId(page, 'boards-source', boardSrc)
    await typeTestId(page, 'boards-target', boardDst)
    await typeTestId(page, 'boards-action', 'copy')
    await clickTestId(page, 'boards-submit')
    await waitForText(page, name)

    await page.evaluate((n: string) => {
      const cards = Array.from(document.querySelectorAll('[data-testid^="board-card-"]'))
      const card = cards.find((c) => c.textContent?.includes(n))
      const buttons = Array.from(card?.querySelectorAll('button') ?? []) as HTMLButtonElement[]
      const run = buttons.find((b) => /Run|Chạy/i.test(b.textContent ?? ''))
      run?.click()
    }, name)

    const deadline = Date.now() + 30_000
    let ok = false
    while (Date.now() < deadline) {
      if (fs.existsSync(path.join(boardDst, 'board.txt'))) {
        ok = true
        break
      }
      await new Promise((r) => setTimeout(r, 400))
    }
    assert.ok(ok, `board execute should copy board.txt into ${boardDst}`)

    await page.evaluate((n: string) => {
      const cards = Array.from(document.querySelectorAll('[data-testid^="board-card-"]'))
      const card = cards.find((c) => c.textContent?.includes(n))
      const buttons = Array.from(card?.querySelectorAll('button') ?? []) as HTMLButtonElement[]
      const del = buttons.find((b) => b.querySelector('svg') && !/Run|Chạy|Stop|Dừng/i.test(b.textContent ?? ''))
      // last danger button
      buttons[buttons.length - 1]?.click()
      void del
    }, name)
    await confirmDialog(page, 'Delete')
    await textAbsent(page, name)
    await collectCoverage(page)
    try {
      fs.rmSync(boardSrc, { recursive: true, force: true })
      fs.rmSync(boardDst, { recursive: true, force: true })
    } catch {
      // ignore
    }
  })

  it('creates and deletes a flow', async () => {
    const name = `e2e-flow-${Date.now()}`
    await waitForTestId(page, 'page-workspace')
    await clickTestId(page, 'flows-add')
    await waitForTestId(page, 'flows-add-form')
    await typeTestId(page, 'flows-name', name)
    await typeTestId(page, 'flows-cron', '0 * * * *')
    await clickTestId(page, 'flows-submit')
    await waitForText(page, name)
    await page.evaluate((n: string) => {
      const cards = Array.from(document.querySelectorAll('[data-testid^="flow-card-"]'))
      const card = cards.find((c) => c.textContent?.includes(n))
      const btn = card?.querySelector('button[data-testid^="flows-delete-"]') as HTMLButtonElement | null
      btn?.click()
    }, name)
    await confirmDialog(page, 'Delete')
    await textAbsent(page, name)
    await collectCoverage(page)
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
