<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount, tick } from "svelte";
  import projectIcon from "../src-tauri/icons/128x128.png";

  type SearchItem = {
    id: number;
    kind: "phrase" | "word";
    title: string;
    shortcut: string;
    body: string;
    category: string;
  };

  let items: SearchItem[] = [];
  let query = "";
  let selectedIndex = 0;
  let searchInput: HTMLInputElement;
  let loading = true;
  let error = "";
  let insertionError = "";
  let inserting = false;
  let resultsElement: HTMLElement;
  $: scrollSelection(selectedIndex, results);
  async function scrollSelection(index: number, _results: SearchItem[]) {
    await tick();
    resultsElement?.querySelectorAll("button")[index]?.scrollIntoView({ block: "nearest" });
  }

  $: normalizedQuery = query.trim().toLocaleLowerCase("ru");
  $: results = normalizedQuery
    ? items.filter((item) =>
        [item.title, item.shortcut, item.body, item.category].some(
          (value) => value.toLocaleLowerCase("ru").includes(normalizedQuery),
        ),
      )
    : items;
  $: if (selectedIndex >= results.length) selectedIndex = Math.max(0, results.length - 1);

  async function openSearch() {
    query = "";
    selectedIndex = 0;
    error = "";
    insertionError = "";
    loading = true;

    try {
      items = await invoke<SearchItem[]>("quick_search_items");
    } catch (cause) {
      error = String(cause);
    } finally {
      loading = false;
      await tick();
      searchInput?.focus();
    }
  }

  async function closeSearch() {
    await invoke("close_quick_search");
  }

  async function insertItem(item: SearchItem | undefined) {
    if (!item || inserting) return;
    insertionError = "";
    inserting = true;
    try {
      await invoke("insert_quick_search_item", { kind: item.kind, itemId: item.id });
    } catch (cause) {
      insertionError = "Не удалось вставить текст: " + String(cause);
    } finally { inserting = false; }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      void closeSearch();
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      selectedIndex = Math.max(0, Math.min(selectedIndex + 1, results.length - 1));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      selectedIndex = Math.max(selectedIndex - 1, 0);
    } else if (event.key === "Enter") {
      event.preventDefault();
      void insertItem(results[selectedIndex]);
    }
  }

  onMount(() => {
    let unlisten: UnlistenFn | undefined;
    void (async () => {
      unlisten = await listen("quick-search-opened", openSearch);
    })();

    return () => unlisten?.();
  });
</script>

<svelte:window onkeydown={handleKeydown} />

<main class="search-shell">
  <header>
    <img class="project-icon" src={projectIcon} alt="" width="34" height="34" />
    <div>
      <strong>Быстрый поиск</strong>
      <span>Фразы и слова активного профиля</span>
    </div>
    <kbd>Esc</kbd>
  </header>

  <div class="search-field">
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="11" cy="11" r="7"></circle>
      <path d="m16.5 16.5 4 4"></path>
    </svg>
    <input
      aria-label="Поиск фраз и слов"
      bind:this={searchInput}
      bind:value={query}
      oninput={() => { selectedIndex = 0; insertionError = ""; }}
      placeholder="Фраза, сокращение, слово, текст или категория"
      autocomplete="off"
      spellcheck="false"
    />
    <span class="result-count">{results.length}</span>
  </div>

  <section class="results" bind:this={resultsElement}>
    {#if loading}
      <div class="empty">Загружаю фразы...</div>
    {:else if error}
      <div class="empty error">{error}</div>
    {:else if results.length === 0}
      <div class="empty">
        <strong>Ничего не найдено</strong>
        <span>Попробуйте другое слово или сокращение</span>
      </div>
    {:else}
      {#each results as item, index (`${item.kind}-${item.id}`)}
        <button
          class:selected={index === selectedIndex}
          onmouseenter={() => (selectedIndex = index)}
          onclick={() => insertItem(item)}
        >
          <span class="phrase-main">
            <strong>{item.title}</strong>
            <small>
              {item.kind === "word"
                ? "Слово из пользовательского словаря"
                : item.body.replace(/\s+/g, " ")}
            </small>
          </span>
          <span class="phrase-meta">
            {#if item.category}<span>{item.category}</span>{/if}
            <code class:word-badge={item.kind === "word"}>{item.shortcut}</code>
          </span>
        </button>
      {/each}
    {/if}
  </section>

  {#if insertionError}<p class="insertion-error" role="alert">{insertionError}</p>{/if}
  <footer>
    <span><kbd>↑</kbd><kbd>↓</kbd> выбор</span>
    <span><kbd>Enter</kbd> вставить</span>
  </footer>
</main>
