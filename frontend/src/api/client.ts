// Minimal fetch-based API client. Uses cookies for session auth.
// Vite dev server proxies /api/* to the Go backend.

export interface ApiError extends Error {
  status: number
  code?: string
  body?: unknown
}

function makeError(status: number, body: unknown, message: string): ApiError {
  const err = new Error(message) as ApiError
  err.status = status
  err.code = (body as any)?.code
  err.body = body
  return err
}

export const api = {
  async get<T = unknown>(path: string): Promise<T> {
    const r = await fetch(path, { credentials: 'same-origin' })
    return handle<T>(r)
  },
  async post<T = unknown>(path: string, body?: unknown): Promise<T> {
    const r = await fetch(path, {
      method: 'POST',
      credentials: 'same-origin',
      headers: { 'Content-Type': 'application/json' },
      body: body !== undefined ? JSON.stringify(body) : undefined,
    })
    return handle<T>(r)
  },
  async put<T = unknown>(path: string, body?: unknown): Promise<T> {
    const r = await fetch(path, {
      method: 'PUT',
      credentials: 'same-origin',
      headers: { 'Content-Type': 'application/json' },
      body: body !== undefined ? JSON.stringify(body) : undefined,
    })
    return handle<T>(r)
  },
  async delete<T = unknown>(path: string): Promise<T> {
    const r = await fetch(path, { method: 'DELETE', credentials: 'same-origin' })
    return handle<T>(r)
  },
}

async function handle<T>(r: Response): Promise<T> {
  const text = await r.text()
  let body: unknown = null
  if (text) {
    try { body = JSON.parse(text) } catch { body = text }
  }
  if (!r.ok) {
    const msg = (body as any)?.error ?? r.statusText ?? `HTTP ${r.status}`
    throw makeError(r.status, body, msg)
  }
  return body as T
}
