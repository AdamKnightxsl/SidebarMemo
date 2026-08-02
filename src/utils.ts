/** 判断键盘事件是否处于 IME 输入法组词状态 */
export function isComposing(e: KeyboardEvent): boolean {
  return e.isComposing || e.keyCode === 229;
}

/**
 * 针对 Tauri setup 竞态的重试包装：
 * 当 invoke 报 "state not managed" 时，每 interval 毫秒重试一次，最多 maxRetries 次。
 * 其他错误直接抛出，不重试。
 */
export async function invokeWithRetry<T>(
  fn: () => Promise<T>,
  { maxRetries = 10, interval = 300 }: { maxRetries?: number; interval?: number } = {}
): Promise<T> {
  let lastErr: unknown;
  for (let i = 0; i <= maxRetries; i++) {
    try {
      return await fn();
    } catch (e) {
      lastErr = e;
      const msg = String(e);
      if (!msg.includes("state not managed")) throw e;
      if (i < maxRetries) {
        await new Promise((r) => setTimeout(r, interval));
      }
    }
  }
  throw lastErr;
}
