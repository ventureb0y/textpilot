<script lang="ts">
  import Icon from "./Icon.svelte";
  let { path }: { path: string } = $props();
  let name = $derived(path.split("/").at(-1) ?? path);
</script>

<span class="category-label" title={path} aria-label={`Категория: ${path}`}>
  <Icon name="folder" size={13} />
  <span>{name}</span>
</span>

<style>
  .category-label {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
    max-width: 100%;
    color: #65756b;
    font-size: var(--text-secondary);
    font-weight: 400;
    line-height: 1.4;
    vertical-align: middle;
  }
  .category-label :global(svg) { flex-shrink: 0; color: #82968a; }
  .category-label > span { overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
</style>
