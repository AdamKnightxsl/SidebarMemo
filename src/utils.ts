/** 判断键盘事件是否处于 IME 输入法组词状态 */
export function isComposing(e: KeyboardEvent): boolean {
  return e.isComposing || e.keyCode === 229;
}
