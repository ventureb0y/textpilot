type DialogOptions = { onClose: () => void; busy?: boolean; initialFocus?: string };
const stack: HTMLElement[] = [];
const focusable = 'button:not(:disabled), input:not(:disabled), textarea:not(:disabled), select:not(:disabled), a[href], summary, [tabindex]:not([tabindex="-1"])';

/** Shared modal lifecycle. Only the topmost dialog handles focus and Escape. */
export function dialog(node: HTMLElement, options: DialogOptions) {
  const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  stack.push(node);
  node.tabIndex = -1;
  const elements = () => Array.from(node.querySelectorAll<HTMLElement>(focusable)).filter(el => el.getClientRects().length && !el.closest('[inert]'));
  const first = () => node.querySelector<HTMLElement>(options.initialFocus ?? 'input:not(:disabled), textarea:not(:disabled)') ?? elements()[0] ?? node;
  const isTop = () => stack.at(-1) === node;
  queueMicrotask(() => { if (isTop()) first().focus(); });
  function keydown(event: KeyboardEvent) {
    if (!isTop() || event.defaultPrevented) return;
    if (event.key === 'Escape') {
      event.preventDefault(); event.stopImmediatePropagation();
      if (!options.busy) options.onClose();
    } else if (event.key === 'Tab') {
      const targets = elements();
      const index = targets.indexOf(document.activeElement as HTMLElement);
      if (!targets.length || (event.shiftKey ? index <= 0 : index === targets.length - 1 || index < 0)) {
        event.preventDefault();
        (event.shiftKey ? targets.at(-1) ?? node : targets[0] ?? node).focus();
      }
    }
  }
  function focusin(event: FocusEvent) {
    if (isTop() && !node.contains(event.target as Node)) first().focus();
  }
  document.addEventListener('keydown', keydown);
  document.addEventListener('focusin', focusin);
  return {
    update(next: DialogOptions) { options = next; },
    destroy() {
      stack.splice(stack.indexOf(node), 1);
      document.removeEventListener('keydown', keydown);
      document.removeEventListener('focusin', focusin);
      queueMicrotask(() => {
        const top = stack.at(-1);
        if (opener?.isConnected && (!top || top.contains(opener))) opener.focus();
      });
    }
  };
}
