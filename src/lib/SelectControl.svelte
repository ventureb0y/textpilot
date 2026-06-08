<script lang="ts">
  import Icon from "./Icon.svelte";

  type SelectOption = {
    value: string;
    label: string;
    disabled?: boolean;
  };

  let {
    value = $bindable(),
    options,
    disabled = false,
    ariaLabel = "Выбор значения",
  }: {
    value: string;
    options: SelectOption[];
    disabled?: boolean;
    ariaLabel?: string;
  } = $props();

  let open = $state(false);
  let highlightedIndex = $state(0);
  let trigger: HTMLButtonElement;
  let selectedOption = $derived(options.find((option) => option.value === value) ?? options[0]);

  function enabledIndex(start: number, direction: 1 | -1) {
    if (options.length === 0) return -1;

    let index = start;
    for (let attempt = 0; attempt < options.length; attempt += 1) {
      index = (index + direction + options.length) % options.length;
      if (!options[index]?.disabled) return index;
    }
    return -1;
  }

  function openMenu(direction: 1 | -1 = 1) {
    if (disabled || options.length === 0) return;
    const selectedIndex = options.findIndex((option) => option.value === value);
    highlightedIndex =
      selectedIndex >= 0 && !options[selectedIndex]?.disabled
        ? selectedIndex
        : enabledIndex(direction === 1 ? -1 : 0, direction);
    open = true;
  }

  function toggleMenu() {
    if (open) {
      open = false;
    } else {
      openMenu();
    }
  }

  function choose(index: number) {
    const option = options[index];
    if (!option || option.disabled) return;
    value = option.value;
    highlightedIndex = index;
    open = false;
    trigger?.focus();
  }

  function handleKeydown(event: KeyboardEvent) {
    if (disabled) return;

    if (!open) {
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        openMenu(event.key === "ArrowDown" ? 1 : -1);
      }
      return;
    }

    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const next = enabledIndex(highlightedIndex, event.key === "ArrowDown" ? 1 : -1);
      if (next >= 0) highlightedIndex = next;
    } else if (event.key === "Home" || event.key === "End") {
      event.preventDefault();
      const start = event.key === "Home" ? options.length - 1 : 0;
      const next = enabledIndex(start, event.key === "Home" ? 1 : -1);
      if (next >= 0) highlightedIndex = next;
    } else if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      choose(highlightedIndex);
    } else if (event.key === "Escape" || event.key === "Tab") {
      open = false;
    }
  }
</script>

<svelte:window onclick={() => (open = false)} />

<div class="select-control">
  <button
    bind:this={trigger}
    type="button"
    class:open
    class="select-trigger"
    aria-label={ariaLabel}
    aria-haspopup="listbox"
    aria-expanded={open}
    {disabled}
    onclick={(event) => {
      event.stopPropagation();
      toggleMenu();
    }}
    onkeydown={handleKeydown}
  >
    <span>{selectedOption?.label ?? "Выберите значение"}</span>
    <Icon name="chevron" size={15} />
  </button>

  {#if open}
    <div
      class="select-menu"
      role="listbox"
      aria-label={ariaLabel}
      tabindex="-1"
    >
      {#each options as option, index}
        <button
          type="button"
          role="option"
          aria-selected={option.value === value}
          class:selected={option.value === value}
          class:highlighted={index === highlightedIndex}
          disabled={option.disabled}
          onmouseenter={() => {
            if (!option.disabled) highlightedIndex = index;
          }}
          onclick={() => choose(index)}
        >
          <span>{option.label}</span>
          {#if option.value === value}
            <Icon name="check" size={14} />
          {/if}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .select-control {
    position: relative;
    width: 100%;
  }

  .select-trigger {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    width: 100%;
    padding: 9px 10px 9px 11px;
    color: #29362e;
    font: inherit;
    font-size: 11.5px;
    text-align: left;
    border: 1px solid #d7ddd8;
    border-radius: 9px;
    outline: none;
    background: #fbfcfb;
    transition:
      border-color 120ms ease,
      box-shadow 120ms ease,
      background 120ms ease;
  }

  .select-trigger:hover:not(:disabled) {
    border-color: #bdc9c1;
    background: #fff;
  }

  .select-trigger:focus-visible,
  .select-trigger.open {
    border-color: #79ae90;
    box-shadow: 0 0 0 3px rgba(63, 139, 96, 0.08);
    background: #fff;
  }

  .select-trigger:disabled {
    cursor: default;
    opacity: 0.55;
  }

  .select-trigger > span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .select-trigger :global(svg) {
    flex: 0 0 auto;
    color: #87928b;
    transform: rotate(90deg);
    transition: transform 120ms ease;
  }

  .select-trigger.open :global(svg) {
    transform: rotate(-90deg);
  }

  .select-menu {
    position: absolute;
    z-index: 40;
    top: calc(100% + 6px);
    right: 0;
    left: 0;
    display: grid;
    gap: 2px;
    max-height: 220px;
    padding: 5px;
    overflow-y: auto;
    border: 1px solid #d8ded9;
    border-radius: 10px;
    background: #fff;
    box-shadow: 0 18px 42px rgba(22, 38, 29, 0.17);
  }

  .select-menu button {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 18px;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 8px 9px;
    color: #445149;
    font: inherit;
    font-size: 10.5px;
    text-align: left;
    border: 0;
    border-radius: 7px;
    background: transparent;
  }

  .select-menu button:hover,
  .select-menu button.highlighted {
    color: #285f43;
    background: #eef5f0;
  }

  .select-menu button.selected {
    color: #2f7250;
    font-weight: 650;
    background: #e7f2eb;
  }

  .select-menu button:disabled {
    cursor: default;
    opacity: 0.45;
  }

  .select-menu button > span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .select-menu button :global(svg) {
    color: #3c8a60;
  }
</style>
