import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import puppeteer, { type Browser, type BrowserContext, type Page } from 'puppeteer'
import { loadRuntime } from './env.js'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const coverageDir = path.resolve(__dirname, '../../.nyc_output')

let browser: Browser | null = null
let coverageSeq = 0

export async function getBrowser(): Promise<Browser> {
  if (browser) return browser
  browser = await puppeteer.launch({
    headless: process.env.HEADED === '1' ? false : true,
    args: ['--no-sandbox', '--disable-setuid-sandbox', '--disable-dev-shm-usage'],
    defaultViewport: { width: 1280, height: 800 },
    protocolTimeout: 120_000,
  })
  return browser
}

/** Isolated context per test so cookies/sessions do not leak. */
export async function newPage(): Promise<Page> {
  const b = await getBrowser()
  const ctx: BrowserContext = await b.createBrowserContext()
  const page = await ctx.newPage()
  page.setDefaultTimeout(20_000)
  page.setDefaultNavigationTimeout(30_000)
  ;(page as any).__e2eContext = ctx
  return page
}

export async function closePage(page: Page): Promise<void> {
  const ctx = (page as any).__e2eContext as BrowserContext | undefined
  try {
    await page.close()
  } catch {
    // ignore
  }
  if (ctx) {
    try {
      await ctx.close()
    } catch {
      // ignore
    }
  }
}

export async function collectCoverage(page: Page): Promise<void> {
  const { coverage } = loadRuntime()
  if (!coverage) return
  try {
    const cov = await page.evaluate(() => (window as any).__coverage__ ?? null)
    if (!cov) return
    fs.mkdirSync(coverageDir, { recursive: true })
    const file = path.join(coverageDir, `coverage-${process.pid}-${coverageSeq++}.json`)
    fs.writeFileSync(file, JSON.stringify(cov))
  } catch {
    // page may be closed
  }
}

export async function closeBrowser(): Promise<void> {
  if (browser) {
    await browser.close()
    browser = null
  }
}

export async function goto(page: Page, route = '/'): Promise<void> {
  const { baseURL } = loadRuntime()
  const url = route.startsWith('http') ? route : `${baseURL}${route.startsWith('/') ? route : `/${route}`}`
  await page.goto(url, { waitUntil: 'domcontentloaded' })
  await page
    .waitForFunction(() => !!document.querySelector('#app')?.children.length, { timeout: 15_000 })
    .catch(() => undefined)
}

export async function waitForTestId(page: Page, testId: string, timeout = 15_000): Promise<void> {
  await page.waitForSelector(`[data-testid="${testId}"]`, { timeout })
}

export async function clickTestId(page: Page, testId: string): Promise<void> {
  await waitForTestId(page, testId)
  await page.click(`[data-testid="${testId}"]`)
}

/**
 * Set input/select value in a Vue-friendly way (native setter + input/change events).
 * Avoids page.type append bugs when the field already has a default value.
 */
export async function typeTestId(page: Page, testId: string, value: string): Promise<void> {
  await waitForTestId(page, testId)
  const sel = `[data-testid="${testId}"]`
  const tag = await page.$eval(sel, (el) => el.tagName.toLowerCase())
  if (tag === 'select') {
    await page.select(sel, value)
    return
  }
  await page.focus(sel)
  await page.evaluate(
    (selector, val) => {
      const el = document.querySelector(selector) as HTMLInputElement | HTMLTextAreaElement | null
      if (!el) throw new Error(`missing ${selector}`)
      el.focus()
      const proto =
        el instanceof HTMLTextAreaElement
          ? window.HTMLTextAreaElement.prototype
          : window.HTMLInputElement.prototype
      const desc = Object.getOwnPropertyDescriptor(proto, 'value')
      desc?.set?.call(el, val)
      el.dispatchEvent(new Event('input', { bubbles: true }))
      el.dispatchEvent(new Event('change', { bubbles: true }))
    },
    sel,
    value,
  )
}

/**
 * Confirm shared ConfirmDialog by exact button label (trimmed).
 * Avoid matching longer triggers like "Clear all" when looking for "Clear",
 * or "Lock app" when looking for "Lock".
 */
export async function confirmDialog(page: Page, confirmLabel = 'Delete'): Promise<void> {
  await page.waitForFunction(
    (label: string) => {
      const buttons = Array.from(document.querySelectorAll('button'))
      return buttons.some((b) => {
        const t = (b.textContent ?? '').replace(/\s+/g, ' ').trim()
        return t === label
      })
    },
    { timeout: 10_000 },
    confirmLabel,
  )
  await page.evaluate((label: string) => {
    const buttons = Array.from(document.querySelectorAll('button'))
    const btn = buttons.find((b) => {
      const t = (b.textContent ?? '').replace(/\s+/g, ' ').trim()
      return t === label
    })
    if (!btn) throw new Error(`confirm button "${label}" not found`)
    ;(btn as HTMLButtonElement).click()
  }, confirmLabel)
}

export async function waitForText(page: Page, text: string, timeout = 10_000): Promise<void> {
  await page.waitForFunction(
    (t: string) => document.body.innerText.includes(t),
    { timeout },
    text,
  )
}

export async function textAbsent(page: Page, text: string, timeout = 10_000): Promise<void> {
  await page.waitForFunction(
    (t: string) => !document.body.innerText.includes(t),
    { timeout },
    text,
  )
}
