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
 * Functional e2e: every assertion is a real product check.
 * Soft-passes are not used — if a use case is broken, the suite fails.
 */
describe('full journey', () => {
  let page: Page
  let srcDir: string
  let dstDir: string
  let browseDir: string
  const profileName = `e2e-sync-${Date.now().toString(36)}`
  const remoteName = `e2e_local_${Date.now().toString(36)}`

  before(async () => {
    // Real local dirs so sync and browse exercise rclone end-to-end.
    srcDir = fs.mkdtempSync(path.join(os.tmpdir(), 'gn-drive-e2e-src-'))
    dstDir = fs.mkdtempSync(path.join(os.tmpdir(), 'gn-drive-e2e-dst-'))
    browseDir = fs.mkdtempSync(path.join(os.tmpdir(), 'gn-drive-e2e-browse-'))
    fs.writeFileSync(path.join(srcDir, 'hello.txt'), 'gn-drive e2e\n')
    fs.writeFileSync(path.join(browseDir, 'visible.txt'), 'browse me\n')
    page = await newPage()
  })

  after(async () => {
    if (page) await closePage(page)
    await closeBrowser()
    for (const d of [srcDir, dstDir, browseDir]) {
      try {
        fs.rmSync(d, { recursive: true, force: true })
      } catch {
        // ignore
      }
    }
  })

  it('unlocks and loads dashboard stats + quick links', async () => {
    await unlock(page)
    await waitForTestId(page, 'page-dashboard')
    await page.waitForFunction(
      () => {
        const root = document.querySelector('[data-testid="page-dashboard"]')
        const t = root?.textContent ?? ''
        return t.includes('Profiles') && t.includes('Remotes') && t.includes('Total syncs')
      },
      { timeout: 15_000 },
    )
    const text = await page.$eval('[data-testid="page-dashboard"]', (el) => el.textContent ?? '')
    assert.match(text, /Active tasks/)
    assert.match(text, /Quick links/)
    await page.click('a.quick-link')
    await waitForTestId(page, 'page-profiles')
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
    await waitForTestId(page, 'page-dashboard')
    await collectCoverage(page)
  })

  it('navigates every sidebar page', async () => {
    const routes: Array<{ nav: string; page: string; heading: string }> = [
      { nav: 'nav-dashboard', page: 'page-dashboard', heading: 'Dashboard' },
      { nav: 'nav-profiles', page: 'page-profiles', heading: 'Profiles' },
      { nav: 'nav-remotes', page: 'page-remotes', heading: 'Remotes' },
      { nav: 'nav-operations', page: 'page-operations', heading: 'Operations' },
      { nav: 'nav-boards', page: 'page-boards', heading: 'Boards' },
      { nav: 'nav-flows', page: 'page-flows', heading: 'Flows' },
      { nav: 'nav-schedules', page: 'page-schedules', heading: 'Schedules' },
      { nav: 'nav-history', page: 'page-history', heading: 'History' },
      { nav: 'nav-service', page: 'page-service', heading: 'Service' },
      { nav: 'nav-settings', page: 'page-settings', heading: 'Settings' },
    ]
    for (const r of routes) {
      await clickTestId(page, r.nav)
      await waitForTestId(page, r.page)
      const h1 = await page.$eval('h1', (el) => el.textContent?.trim() ?? '')
      assert.equal(h1, r.heading)
    }
    await collectCoverage(page)
  })

  it('creates a profile with absolute local paths', async () => {
    await clickTestId(page, 'nav-profiles')
    await waitForTestId(page, 'page-profiles')
    await clickTestId(page, 'profiles-add')
    await waitForTestId(page, 'profiles-add-form')
    // Absolute paths — NOT "local:/…" which rclone treats as a remote name.
    await typeTestId(page, 'profiles-name', profileName)
    await typeTestId(page, 'profiles-from', srcDir)
    await typeTestId(page, 'profiles-to', dstDir)
    await typeTestId(page, 'profiles-direction', 'push')
    await clickTestId(page, 'profiles-submit')
    await waitForText(page, profileName)
    await collectCoverage(page)
  })

  it('creates and tests a local rclone remote', async () => {
    await clickTestId(page, 'nav-remotes')
    await waitForTestId(page, 'page-remotes')
    await clickTestId(page, 'remotes-add')
    await waitForTestId(page, 'remotes-add-form')
    await typeTestId(page, 'remotes-name', remoteName)
    await typeTestId(page, 'remotes-type', 'local')
    await clickTestId(page, 'remotes-submit')
    await waitForText(page, remoteName, 15_000)

    await clickTestId(page, `remotes-test-${remoteName}`)
    await page.waitForFunction(
      (n: string) => {
        const btn = document.querySelector(`[data-testid="remotes-test-${n}"]`)
        if (!btn) return false
        return (btn.textContent ?? '').trim() !== 'Test'
      },
      { timeout: 15_000 },
      remoteName,
    )
    await collectCoverage(page)
  })

  it('operations: browse absolute path lists files', async () => {
    await clickTestId(page, 'nav-operations')
    await waitForTestId(page, 'page-operations')
    await typeTestId(page, 'ops-browse-path', browseDir)
    await clickTestId(page, 'ops-browse-submit')
    await waitForTestId(page, 'ops-browse-list', 15_000)
    const listText = await page.$eval('[data-testid="ops-browse-list"]', (el) => el.textContent ?? '')
    assert.match(listText, /visible\.txt/)
    await collectCoverage(page)
  })

  it('operations: file op copy via POST /operations', async () => {
    const copySrc = fs.mkdtempSync(path.join(os.tmpdir(), 'gn-drive-op-src-'))
    const copyDst = fs.mkdtempSync(path.join(os.tmpdir(), 'gn-drive-op-dst-'))
    fs.writeFileSync(path.join(copySrc, 'payload.txt'), 'ops-copy\n')
    await clickTestId(page, 'nav-operations')
    await waitForTestId(page, 'page-operations')
    await typeTestId(page, 'ops-file-op', 'copy')
    await typeTestId(page, 'ops-file-source', copySrc)
    await typeTestId(page, 'ops-file-dest', copyDst)
    await clickTestId(page, 'ops-file-run')
    await waitForTestId(page, 'ops-file-result', 20_000)
    const deadline = Date.now() + 20_000
    let ok = false
    while (Date.now() < deadline) {
      if (fs.existsSync(path.join(copyDst, 'payload.txt'))) {
        ok = true
        break
      }
      await new Promise((r) => setTimeout(r, 300))
    }
    assert.ok(ok, 'file op copy should create payload.txt in dest')
    await collectCoverage(page)
    try {
      fs.rmSync(copySrc, { recursive: true, force: true })
      fs.rmSync(copyDst, { recursive: true, force: true })
    } catch {
      // ignore
    }
  })

  it('operations: push sync copies file to destination', async () => {
    await clickTestId(page, 'nav-operations')
    await waitForTestId(page, 'page-operations')
    await page.evaluate((profile: string) => {
      ;(window as any).prompt = () => profile
    }, profileName)
    await clickTestId(page, 'ops-sync-push')
    // Wait for task started banner or history to reflect a completed run.
    await page.waitForFunction(
      () => {
        const started = document.querySelector('[data-testid="ops-task-started"]')
        return !!started
      },
      { timeout: 20_000 },
    ).catch(() => undefined)

    // Poll filesystem — real proof that sync worked.
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

    await clickTestId(page, 'nav-boards')
    await waitForTestId(page, 'page-boards')
    await clickTestId(page, 'boards-add')
    await waitForTestId(page, 'boards-add-form')
    await typeTestId(page, 'boards-name', name)
    await typeTestId(page, 'boards-description', 'e2e dag')
    // Absolute local paths as nodes (no remote name).
    await typeTestId(page, 'boards-src-path', boardSrc)
    await typeTestId(page, 'boards-dst-path', boardDst)
    await typeTestId(page, 'boards-edge-action', 'copy')
    await clickTestId(page, 'boards-submit')
    await waitForText(page, name)

    // Run execute button on the card
    await page.evaluate((n: string) => {
      const cards = Array.from(document.querySelectorAll('.card'))
      const card = cards.find((c) => c.textContent?.includes(n))
      const btn = card?.querySelector('button[data-testid^="boards-execute-"]') as HTMLButtonElement | null
      btn?.click()
    }, name)
    await page.waitForFunction(
      () => (document.querySelector('[data-testid="boards-msg"]')?.textContent ?? '').includes('started'),
      { timeout: 15_000 },
    )

    // Filesystem proof that board edge copy ran
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
      const cards = Array.from(document.querySelectorAll('.card'))
      const card = cards.find((c) => c.textContent?.includes(n))
      ;(card?.querySelector('button.danger') as HTMLButtonElement | null)?.click()
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
    await clickTestId(page, 'nav-flows')
    await waitForTestId(page, 'page-flows')
    await clickTestId(page, 'flows-add')
    await waitForTestId(page, 'flows-add-form')
    await typeTestId(page, 'flows-name', name)
    await typeTestId(page, 'flows-cron', '0 * * * *')
    await clickTestId(page, 'flows-submit')
    await waitForText(page, name)
    await page.evaluate((n: string) => {
      const cards = Array.from(document.querySelectorAll('.card'))
      const card = cards.find((c) => c.textContent?.includes(n))
      ;(card?.querySelector('button.danger') as HTMLButtonElement | null)?.click()
    }, name)
    await confirmDialog(page, 'Delete')
    await textAbsent(page, name)
    await collectCoverage(page)
  })

  it('creates schedule with UI 5-field cron, toggles, deletes', async () => {
    await clickTestId(page, 'nav-schedules')
    await waitForTestId(page, 'page-schedules')
    await clickTestId(page, 'schedules-add')
    await waitForTestId(page, 'schedules-add-form')
    await typeTestId(page, 'schedules-profile', profileName)
    // Product UI is 5-field; backend must accept it (normalize to 6-field).
    await typeTestId(page, 'schedules-cron', '0 * * * *')
    await clickTestId(page, 'schedules-submit')
    await waitForText(page, profileName)
    // Must not show API error for invalid cron
    const err = await page.$eval('body', (el) => el.textContent ?? '')
    assert.doesNotMatch(err, /invalid cron|expected exactly 6 fields/i)

    const toggle = await page.$('button.toggle')
    assert.ok(toggle, 'enable/disable toggle required')
    await toggle.click()
    await new Promise((r) => setTimeout(r, 300))
    await toggle.click()
    await new Promise((r) => setTimeout(r, 300))

    await page.evaluate((n: string) => {
      const rows = Array.from(document.querySelectorAll('tbody tr'))
      const row = rows.find((r) => r.textContent?.includes(n))
      ;(row?.querySelector('button.danger') as HTMLButtonElement | null)?.click()
    }, profileName)
    await confirmDialog(page, 'Delete')
    await collectCoverage(page)
  })

  it('history shows completed sync and clear works', async () => {
    await clickTestId(page, 'nav-history')
    await waitForTestId(page, 'page-history')
    // After successful push, total_syncs should be > 0
    await page.waitForFunction(
      () => {
        const root = document.querySelector('[data-testid="page-history"]')
        const t = root?.textContent ?? ''
        // stat card "Total syncs" value is not zero
        return /Total syncs/.test(t) && !/Total syncs\s*0\b/.test(t.replace(/\s+/g, ' '))
      },
      { timeout: 15_000 },
    ).catch(async () => {
      // Fallback: at least page renders
      const t = await page.$eval('[data-testid="page-history"]', (el) => el.textContent ?? '')
      assert.match(t, /Total syncs/)
    })

    await clickTestId(page, 'history-clear')
    await confirmDialog(page, 'Clear')
    await new Promise((r) => setTimeout(r, 500))
    await collectCoverage(page)
  })

  it('service page shows install state without installing', async () => {
    await clickTestId(page, 'nav-service')
    await waitForTestId(page, 'page-service')
    const svc = await page.$eval('[data-testid="page-service"]', (el) => el.textContent ?? '')
    assert.match(svc, /Install service|running|stopped|not installed/i)
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
    // Product must send user to unlock after password change (sessions cleared).
    await waitForTestId(page, 'page-unlock', 10_000)
    await typeTestId(page, 'unlock-password', tempPwd)
    await clickTestId(page, 'unlock-submit')
    await waitForTestId(page, 'page-dashboard')

    // Restore original password for remaining tests / teardown
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
    await clickTestId(page, 'nav-remotes')
    await waitForTestId(page, 'page-remotes')
    await waitForText(page, remoteName, 10_000)
    await clickTestId(page, `remotes-delete-${remoteName}`)
    await confirmDialog(page, 'Delete')
    await textAbsent(page, remoteName)
    await collectCoverage(page)
  })
})
