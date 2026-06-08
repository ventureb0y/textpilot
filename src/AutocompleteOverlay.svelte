<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  type AutocompleteSuggestion = {
    prefix: string;
    words: string[];
    selectedIndex: number;
  };

  let suggestion: AutocompleteSuggestion | null = null;

  function select(index: number) {
    if (!suggestion || suggestion.selectedIndex === index) return;
    suggestion = { ...suggestion, selectedIndex: index };
    void invoke("select_autocomplete", { index });
  }

  function accept(index: number) {
    void invoke("accept_autocomplete", { index });
  }

  onMount(() => {
    let unlisten: UnlistenFn | undefined;

    void (async () => {
      unlisten = await listen<AutocompleteSuggestion>(
        "autocomplete-suggestion",
        (event) => {
          suggestion = event.payload;
        },
      );
    })();

    return () => unlisten?.();
  });
</script>

{#if suggestion}
  <div class="completion-popup" role="listbox" aria-label="Варианты автодополнения">
    <div class="completion-list">
      {#each suggestion.words as word, index}
        <button
          class:selected={index === suggestion.selectedIndex}
          role="option"
          aria-selected={index === suggestion.selectedIndex}
          onmouseenter={() => select(index)}
          onmousedown={(event) => event.preventDefault()}
          onclick={() => accept(index)}
        >
          <span class="word">
            <span class="prefix">{suggestion.prefix}</span>{word.slice(
              suggestion.prefix.length,
            )}
          </span>
          <span class="kind">Слово</span>
        </button>
      {/each}
    </div>
    <footer>
      <span><kbd>↑</kbd><kbd>↓</kbd> выбор</span>
      <span><kbd>Tab</kbd> вставить</span>
    </footer>
  </div>
{/if}
