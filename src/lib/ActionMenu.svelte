<script lang="ts">
  import { tick } from "svelte";
  import Icon from "./Icon.svelte";
  let { label, items, disabled = false }: {
    label: string;
    items: { label: string; action: () => unknown; danger?: boolean }[];
    disabled?: boolean;
  } = $props();
  let open = $state(false);
  let trigger: HTMLButtonElement;
  let container: HTMLDivElement;
  let panel = $state<HTMLDivElement>(null!);
  let placement = $state("");
  function close() { open = false; trigger?.focus(); }
  async function show(last = false) {
    if (disabled) return;
    open = true;
    await tick();
    const anchor = trigger.getBoundingClientRect();
    const box = panel.getBoundingClientRect();
    const left = Math.max(8, Math.min(anchor.right - box.width, window.innerWidth - box.width - 8));
    const top = anchor.bottom + box.height + 8 <= window.innerHeight ? anchor.bottom + 4 : Math.max(8, anchor.top - box.height - 4);
    placement = `left: ${left}px; top: ${top}px`;
    const buttons = panel.querySelectorAll<HTMLButtonElement>('button');
    (last ? buttons[buttons.length - 1] : buttons[0])?.focus();
  }
  function keydown(event: KeyboardEvent) {
    if (!open) return;
    if (event.key === 'Escape' || event.key === 'Tab') {
      if (event.key === 'Escape') event.preventDefault();
      event.stopPropagation(); close();
    } else if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
      event.preventDefault();
      const buttons = Array.from(panel.querySelectorAll<HTMLButtonElement>('button'));
      const current = buttons.indexOf(document.activeElement as HTMLButtonElement);
      const next = event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length - 1 : (current + (event.key === 'ArrowDown' ? 1 : -1) + buttons.length) % buttons.length;
      buttons[next]?.focus();
    }
  }
</script>

<svelte:window onpointerdown={(event) => { if (open && !container.contains(event.target as Node)) close(); }} onresize={() => { if (open) close(); }} />
<div class="action-menu" bind:this={container}>
  <button bind:this={trigger} type="button" class="icon-button" aria-label={label} aria-haspopup="menu" aria-expanded={open} {disabled}
    onclick={() => open ? close() : show()}
    onkeydown={(event) => { if (event.key === 'ArrowDown' || event.key === 'ArrowUp') { event.preventDefault(); void show(event.key === 'ArrowUp'); } }}>
    <Icon name="more" size={18} />
  </button>
  {#if open}
    <div class="action-menu-panel" bind:this={panel} style={placement} role="menu" aria-label={label} tabindex="-1" onkeydown={keydown}>
      {#each items as item}
        <button type="button" role="menuitem" tabindex="-1" class:danger-action={item.danger} onclick={() => { close(); void item.action(); }}>{item.label}</button>
      {/each}
    </div>
  {/if}
</div>
