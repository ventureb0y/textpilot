import { mount } from "svelte";
import { getCurrentWindow } from "@tauri-apps/api/window";

const target = document.getElementById("app")!;
const windowLabel = getCurrentWindow().label;

if (windowLabel === "autocomplete") {
  await import("./overlay.css");
  const { default: AutocompleteOverlay } = await import(
    "./AutocompleteOverlay.svelte"
  );
  mount(AutocompleteOverlay, { target });
} else if (windowLabel === "quick-search") {
  await import("./quick-search.css");
  const { default: QuickSearch } = await import("./QuickSearch.svelte");
  mount(QuickSearch, { target });
} else {
  await import("./app.css");
  const { default: App } = await import("./App.svelte");
  mount(App, { target });
}
