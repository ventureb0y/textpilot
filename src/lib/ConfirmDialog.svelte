<script lang="ts">
  import Icon from "./Icon.svelte";

  let {
    open,
    eyebrow = "Подтверждение действия",
    title,
    message,
    details = [],
    confirmLabel = "Подтвердить",
    cancelLabel = "Отмена",
    onConfirm,
    onCancel,
  }: {
    open: boolean;
    eyebrow?: string;
    title: string;
    message: string;
    details?: string[];
    confirmLabel?: string;
    cancelLabel?: string;
    onConfirm: () => void;
    onCancel: () => void;
  } = $props();

  let cancelButton = $state<HTMLButtonElement>();
  let confirmButton = $state<HTMLButtonElement>();

  $effect(() => {
    if (open) {
      window.setTimeout(() => cancelButton?.focus(), 0);
    }
  });

  function handleKeydown(event: KeyboardEvent) {
    if (!open) return;

    if (event.key === "Escape") {
      event.preventDefault();
      onCancel();
    } else if (event.key === "Tab") {
      const active = document.activeElement;
      if (event.shiftKey && active === cancelButton) {
        event.preventDefault();
        confirmButton?.focus();
      } else if (!event.shiftKey && active === confirmButton) {
        event.preventDefault();
        cancelButton?.focus();
      }
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
  <div
    class="modal-backdrop confirm-backdrop"
    role="presentation"
    onclick={(event) => event.target === event.currentTarget && onCancel()}
  >
    <div
      class="modal confirm-dialog"
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="confirm-dialog-title"
      aria-describedby="confirm-dialog-message"
    >
      <div class="confirm-icon"><Icon name="trash" size={22} /></div>
      <div class="confirm-copy">
        <span class="page-kicker">{eyebrow}</span>
        <h2 id="confirm-dialog-title">{title}</h2>
        <p id="confirm-dialog-message">{message}</p>

        {#if details.length > 0}
          <div class="confirm-details">
            {#each details as detail}
              <span>{detail}</span>
            {/each}
          </div>
        {/if}
      </div>

      <footer class="confirm-actions">
        <button bind:this={cancelButton} type="button" class="ghost-button" onclick={onCancel}>
          {cancelLabel}
        </button>
        <button
          bind:this={confirmButton}
          type="button"
          class="danger-button confirm-button"
          onclick={onConfirm}
        >
          {confirmLabel}
        </button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .confirm-backdrop {
    z-index: 50;
  }

  .confirm-dialog {
    display: grid;
    grid-template-columns: 46px minmax(0, 1fr);
    gap: 14px;
    width: min(470px, 100%);
    padding: 22px;
    overflow: visible;
    animation: confirm-appear 140ms ease-out;
  }

  .confirm-icon {
    display: grid;
    width: 46px;
    height: 46px;
    color: #a24747;
    place-items: center;
    border: 1px solid #f0d6d6;
    border-radius: 13px;
    background: #fff4f4;
  }

  .confirm-copy {
    min-width: 0;
  }

  .confirm-copy h2 {
    margin: 5px 0 8px;
    color: #26332c;
    font-size: 17px;
    line-height: 1.3;
    letter-spacing: -0.025em;
  }

  .confirm-copy p {
    margin: 0;
    color: #78837c;
    font-size: 10.5px;
    line-height: 1.55;
  }

  .confirm-details {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 13px;
  }

  .confirm-details span {
    padding: 5px 7px;
    color: #766464;
    font-size: 9.5px;
    border: 1px solid #eadede;
    border-radius: 7px;
    background: #fbf7f7;
  }

  .confirm-actions {
    display: flex;
    grid-column: 1 / -1;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
    padding-top: 17px;
    border-top: 1px solid #ecefec;
  }

  .confirm-button {
    padding: 8px 13px;
    color: #fff;
    border-color: #9f3f3f;
    background: #b64d4d;
  }

  .confirm-button:hover {
    color: #fff;
    border-color: #8f3434;
    background: #a83f3f;
  }

  .confirm-button:focus-visible,
  .confirm-actions :global(.ghost-button):focus-visible {
    outline: 3px solid rgba(73, 139, 101, 0.14);
    outline-offset: 2px;
  }

  @keyframes confirm-appear {
    from {
      opacity: 0;
      transform: translateY(5px) scale(0.985);
    }
  }
</style>
