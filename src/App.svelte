<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import ConfirmDialog from "./lib/ConfirmDialog.svelte";
  import Icon from "./lib/Icon.svelte";
  import SelectControl from "./lib/SelectControl.svelte";
  import type {
    BackupSummary,
    CategorySummary,
    DashboardData,
    DictionaryWordSummary,
    ExportResult,
    ImportPreview,
    ImportResult,
    NavigationSection,
    PhraseSummary,
    ProfileSnapshotSummary,
    ProfileSummary,
    RestoreBackupResult,
    RestoreProfileSnapshotResult,
  } from "./lib/types";

  const navItems: Array<{
    id: NavigationSection;
    label: string;
    icon: string;
  }> = [
    { id: "phrases", label: "Фразы", icon: "phrase" },
    { id: "dictionary", label: "Словарь", icon: "book" },
    { id: "profiles", label: "Профили", icon: "users" },
    { id: "settings", label: "Настройки", icon: "settings" },
  ];

  const snippetPattern = /^[\p{L}\p{N}_-]+$/u;

  type ConfirmationState = {
    eyebrow?: string;
    title: string;
    message: string;
    details?: string[];
    confirmLabel?: string;
    resolve: (confirmed: boolean) => void;
  };

  const previewData: DashboardData = {
    profiles: [
      {
        id: 1,
        name: "Основной",
        isActive: true,
        categoryCount: 3,
        phraseCount: 4,
        dictionaryCount: 146,
      },
    ],
    activeProfileId: 1,
    dictionaryCount: 146,
    isPaused: false,
    engineAvailable: true,
    dictionaryAutocompleteEnabled: true,
    quickSearchEnabled: true,
    categories: [
      { id: 1, parentId: null, name: "Продажи", path: "Продажи", phraseCount: 2, wordCount: 3 },
      { id: 2, parentId: null, name: "Поддержка", path: "Поддержка", phraseCount: 1, wordCount: 1 },
      { id: 3, parentId: null, name: "Документы", path: "Документы", phraseCount: 1, wordCount: 0 },
    ],
    phrases: [
      {
        id: 1,
        categoryId: 1,
        title: "Коммерческое предложение",
        snippet: "кп",
        body: "Здравствуйте! Отправляю вам коммерческое предложение. Если возникнут вопросы — напишите, пожалуйста.",
        description: "Первичный ответ клиенту",
        isEnabled: true,
      },
      {
        id: 2,
        categoryId: 1,
        title: "Срок изготовления",
        snippet: "сроки",
        body: "Стандартный срок изготовления — от 3 до 5 рабочих дней после согласования макета.",
        description: "",
        isEnabled: true,
      },
      {
        id: 3,
        categoryId: 2,
        title: "Уточнение проблемы",
        snippet: "уточнить",
        body: "Подскажите, пожалуйста, в какой момент возникает ошибка и какой текст вы видите на экране?",
        description: "",
        isEnabled: true,
      },
      {
        id: 4,
        categoryId: 3,
        title: "Запрос реквизитов",
        snippet: "рекв",
        body: "Пришлите, пожалуйста, карточку организации с актуальными реквизитами.",
        description: "",
        isEnabled: true,
      },
    ],
    dictionaryWords: [
      {
        id: 1,
        categoryId: 1,
        word: "коммерческое",
        priority: 10,
        isEnabled: true,
        autocompleteEnabled: true,
        autocorrectEnabled: true,
      },
      {
        id: 2,
        categoryId: 1,
        word: "предложение",
        priority: 8,
        isEnabled: true,
        autocompleteEnabled: true,
        autocorrectEnabled: true,
      },
    ],
  };

  let data: DashboardData | null = null;
  let activeSection: NavigationSection = "phrases";
  let selectedCategory: number | "all" | "uncategorized" = "all";
  let selectedPhrase: PhraseSummary | null = null;
  let search = "";
  let loading = true;
  let error = "";
  let paused = false;
  let phraseModalOpen = false;
  let categoryModalOpen = false;
  let editingCategoryId: number | null = null;
  let categoryParentId = "";
  let categoryActionError = "";
  let draggedCategoryId: number | null = null;
  let categoryDropTargetId: number | null = null;
  let categoryDropIndicatorId: number | null = null;
  let categoryDropPosition: "before" | "after" | null = null;
  let categoryPointerId: number | null = null;
  let categoryPointerCategoryId: number | null = null;
  let categoryPointerStartX = 0;
  let categoryPointerStartY = 0;
  let categoryPointerDragging = false;
  let suppressCategoryClick = false;
  let reorderingCategories = false;
  let saving = false;
  let formError = "";
  let phraseActionError = "";
  let editingPhraseId: number | null = null;
  let profileModalOpen = false;
  let editingProfileId: number | null = null;
  let profileName = "";
  let profileFormError = "";
  let profileActionError = "";
  let profileDropdownOpen = false;
  let dictionarySearch = "";
  let selectedDictionaryCategory: number | "all" | "uncategorized" = "all";
  let dictionaryModalOpen = false;
  let editingDictionaryWordId: number | null = null;
  let dictionaryWord = "";
  let dictionaryCategoryId = "";
  let dictionaryPriority = 0;
  let dictionaryEnabled = true;
  let dictionaryAutocomplete = true;
  let dictionaryAutocorrect = true;
  let dictionaryFormError = "";
  let dictionaryActionError = "";
  let importFileInput: HTMLInputElement;
  let transferBusy = false;
  let transferError = "";
  let transferSuccess = "";
  let importFileName = "";
  let importJson = "";
  let importPreview: ImportPreview | null = null;
  let importModalOpen = false;
  let importStrategy: "skip" | "overwrite" = "skip";
  let backups: BackupSummary[] = [];
  let backupsLoading = false;
  let profileSnapshots: ProfileSnapshotSummary[] = [];
  let profileSnapshotsLoading = false;
  let dictionaryAutocompleteSaving = false;
  let dictionaryAutocompleteError = "";
  let quickSearchSaving = false;
  let quickSearchError = "";
  let confirmation: ConfirmationState | null = null;

  let categoryName = "";
  let phraseTitle = "";
  let phraseSnippet = "";
  let phraseBody = "";
  let phraseDescription = "";
  let phraseCategoryId = "";

  $: activeProfile = data?.profiles.find((profile) => profile.id === data?.activeProfileId);
  $: selectedCategoryRecord =
    typeof selectedCategory === "number"
      ? data?.categories.find((category) => category.id === selectedCategory) ?? null
      : null;
  $: selectedCategoryIds = selectedCategoryRecord
    ? new Set(
        data?.categories
          .filter(
            (category) =>
              category.path === selectedCategoryRecord?.path ||
              category.path.startsWith(`${selectedCategoryRecord?.path}/`),
          )
          .map((category) => category.id) ?? [],
      )
    : null;
  $: selectedDictionaryCategoryRecord =
    typeof selectedDictionaryCategory === "number"
      ? data?.categories.find((category) => category.id === selectedDictionaryCategory) ?? null
      : null;
  $: selectedDictionaryCategoryIds = selectedDictionaryCategoryRecord
    ? new Set(
        data?.categories
          .filter(
            (category) =>
              category.path === selectedDictionaryCategoryRecord?.path ||
              category.path.startsWith(`${selectedDictionaryCategoryRecord?.path}/`),
          )
          .map((category) => category.id) ?? [],
      )
    : null;
  $: normalizedSearch = search.trim().toLocaleLowerCase("ru");
  $: filteredPhrases =
    data?.phrases.filter((phrase) => {
      const matchesCategory =
        selectedCategory === "all" ||
        (selectedCategory === "uncategorized" && phrase.categoryId === null) ||
        (phrase.categoryId !== null && selectedCategoryIds?.has(phrase.categoryId));
      const matchesSearch =
        !normalizedSearch ||
        [phrase.title, phrase.snippet, phrase.body, phrase.description].some((value) =>
          value.toLocaleLowerCase("ru").includes(normalizedSearch),
        );
      return matchesCategory && matchesSearch;
    }) ?? [];
  $: normalizedDictionarySearch = dictionarySearch.trim().toLocaleLowerCase("ru");
  $: filteredDictionaryWords =
    data?.dictionaryWords.filter((entry) => {
      const matchesCategory =
        selectedDictionaryCategory === "all" ||
        (selectedDictionaryCategory === "uncategorized" && entry.categoryId === null) ||
        (entry.categoryId !== null &&
          selectedDictionaryCategoryIds?.has(entry.categoryId));
      return (
        matchesCategory &&
        (!normalizedDictionarySearch ||
          entry.word.toLocaleLowerCase("ru").includes(normalizedDictionarySearch))
      );
    }) ?? [];

  onMount(() => {
    loadDashboard();
    const inTauri = "__TAURI_INTERNALS__" in window;
    let stopListening: (() => void) | undefined;

    if (inTauri) {
      listen<boolean>("pause-changed", (event) => {
        paused = event.payload;
        data = data ? { ...data, isPaused: paused } : data;
      }).then((unlisten) => {
        stopListening = unlisten;
      });
    } else if (new URLSearchParams(location.search).has("new")) {
      phraseModalOpen = true;
    }

    return () => stopListening?.();
  });

  async function loadDashboard() {
    loading = true;
    error = "";
    if (!("__TAURI_INTERNALS__" in window)) {
      data = previewData;
      loading = false;
      return;
    }

    try {
      data = await loadDashboardWhenReady();
      paused = data.isPaused;
      if (selectedPhrase) {
        selectedPhrase =
          data.phrases.find((phrase) => phrase.id === selectedPhrase?.id) ?? null;
      }
    } catch (cause) {
      error = String(cause);
    } finally {
      loading = false;
    }
  }

  async function loadDashboardWhenReady() {
    const attempts = 40;

    for (let attempt = 0; attempt < attempts; attempt += 1) {
      try {
        return await invoke<DashboardData>("dashboard_data");
      } catch (cause) {
        const message = String(cause);
        if (!message.includes("state not managed") || attempt === attempts - 1) {
          throw cause;
        }
        await new Promise((resolve) => window.setTimeout(resolve, 50));
      }
    }

    throw new Error("TextPilot state initialization timed out");
  }

  async function togglePaused() {
    if (!data?.engineAvailable) return;

    const next = !paused;
    try {
      paused = await invoke<boolean>("set_paused", { paused: next });
      data = data ? { ...data, isPaused: paused } : data;
    } catch (cause) {
      error = String(cause);
    }
  }

  async function toggleQuickSearch() {
    if (!data) return;

    quickSearchSaving = true;
    quickSearchError = "";
    try {
      const enabled = await invoke<boolean>("set_quick_search_enabled", {
        enabled: !data.quickSearchEnabled,
      });
      data = { ...data, quickSearchEnabled: enabled };
    } catch (cause) {
      quickSearchError = `Не удалось изменить настройку: ${String(cause)}`;
    } finally {
      quickSearchSaving = false;
    }
  }

  async function toggleDictionaryAutocomplete() {
    if (!data) return;

    dictionaryAutocompleteSaving = true;
    dictionaryAutocompleteError = "";
    try {
      const enabled = await invoke<boolean>("set_dictionary_autocomplete_enabled", {
        enabled: !data.dictionaryAutocompleteEnabled,
      });
      data = { ...data, dictionaryAutocompleteEnabled: enabled };
    } catch (cause) {
      dictionaryAutocompleteError = "Не удалось изменить настройку: " + String(cause);
    } finally {
      dictionaryAutocompleteSaving = false;
    }
  }

  function openProfileModal(profile: ProfileSummary | null = null) {
    editingProfileId = profile?.id ?? null;
    profileName = profile?.name ?? "";
    profileFormError = "";
    profileModalOpen = true;
  }

  function closeProfileModal() {
    profileModalOpen = false;
    editingProfileId = null;
    profileName = "";
    profileFormError = "";
  }

  async function saveProfile() {
    const name = profileName.trim();
    profileFormError = "";
    if (!name) {
      profileFormError = "Введите название профиля.";
      return;
    }

    saving = true;
    try {
      if (editingProfileId === null) {
        await invoke("create_profile", { name });
        resetProfileWorkspace();
      } else {
        await invoke("rename_profile", {
          profileId: editingProfileId,
          name,
        });
      }
      closeProfileModal();
      await loadDashboard();
    } catch (cause) {
      profileFormError = readableError(cause);
    } finally {
      saving = false;
    }
  }

  async function activateProfile(profile: ProfileSummary) {
    profileDropdownOpen = false;
    if (profile.isActive) return;

    saving = true;
    profileActionError = "";
    try {
      await invoke("set_active_profile", { profileId: profile.id });
      resetProfileWorkspace();
      await loadDashboard();
      if (activeSection === "settings") {
        await loadProfileSnapshots(profile.id);
      }
    } catch (cause) {
      profileActionError = readableError(cause);
    } finally {
      saving = false;
    }
  }

  async function deleteProfile(profile: ProfileSummary) {
    const contents = [
      `${profile.phraseCount} фраз`,
      `${profile.dictionaryCount} слов`,
      `${profile.categoryCount} категорий`,
    ];
    if (
      !(await askForConfirmation({
        eyebrow: "Удаление профиля",
        title: `Удалить «${profile.name}»?`,
        message: "Все данные этого профиля будут удалены без возможности восстановления.",
        details: contents,
        confirmLabel: "Удалить профиль",
      }))
    ) return;

    saving = true;
    profileActionError = "";
    try {
      await invoke("delete_profile", { profileId: profile.id });
      resetProfileWorkspace();
      await loadDashboard();
    } catch (cause) {
      profileActionError = readableError(cause);
    } finally {
      saving = false;
    }
  }

  function resetProfileWorkspace() {
    selectedCategory = "all";
    selectedPhrase = null;
    search = "";
  }

  function openProfileManagement() {
    profileDropdownOpen = false;
    activeSection = "profiles";
  }

  function openDictionaryModal(entry: DictionaryWordSummary | null = null) {
    editingDictionaryWordId = entry?.id ?? null;
    dictionaryWord = entry?.word ?? "";
    dictionaryCategoryId =
      entry?.categoryId === null || entry?.categoryId === undefined
        ? typeof selectedDictionaryCategory === "number"
          ? String(selectedDictionaryCategory)
          : ""
        : String(entry.categoryId);
    dictionaryPriority = entry?.priority ?? 0;
    dictionaryEnabled = entry?.isEnabled ?? true;
    dictionaryAutocomplete = entry?.autocompleteEnabled ?? true;
    dictionaryAutocorrect = entry?.autocorrectEnabled ?? true;
    dictionaryFormError = "";
    dictionaryModalOpen = true;
  }

  function closeDictionaryModal() {
    dictionaryModalOpen = false;
    editingDictionaryWordId = null;
    dictionaryFormError = "";
  }

  async function saveDictionaryWord() {
    const word = dictionaryWord.trim();
    dictionaryFormError = "";
    if (!word || !/^[\p{L}-]+$/u.test(word)) {
      dictionaryFormError = "Слово может содержать только буквы и дефис.";
      return;
    }
    if (dictionaryPriority < 0 || dictionaryPriority > 100) {
      dictionaryFormError = "Приоритет должен быть от 0 до 100.";
      return;
    }

    saving = true;
    try {
      await invoke("save_dictionary_word", {
        input: {
          id: editingDictionaryWordId,
          word,
          categoryId: dictionaryCategoryId ? Number(dictionaryCategoryId) : null,
          priority: Number(dictionaryPriority),
          isEnabled: dictionaryEnabled,
          autocompleteEnabled: dictionaryAutocomplete,
          autocorrectEnabled: dictionaryAutocorrect,
        },
      });
      closeDictionaryModal();
      await loadDashboard();
    } catch (cause) {
      dictionaryFormError = readableError(cause);
    } finally {
      saving = false;
    }
  }

  async function updateDictionaryWord(
    entry: DictionaryWordSummary,
    changes: Partial<DictionaryWordSummary>,
  ) {
    saving = true;
    dictionaryActionError = "";
    try {
      const updated = { ...entry, ...changes };
      await invoke("save_dictionary_word", {
        input: {
          id: updated.id,
          word: updated.word,
          categoryId: updated.categoryId,
          priority: updated.priority,
          isEnabled: updated.isEnabled,
          autocompleteEnabled: updated.autocompleteEnabled,
          autocorrectEnabled: updated.autocorrectEnabled,
        },
      });
      await loadDashboard();
    } catch (cause) {
      dictionaryActionError = readableError(cause);
    } finally {
      saving = false;
    }
  }

  async function deleteDictionaryWord(entry: DictionaryWordSummary) {
    if (
      !(await askForConfirmation({
        eyebrow: "Удаление из словаря",
        title: `Удалить слово «${entry.word}»?`,
        message:
          "Слово перестанет участвовать в автокомплите и автокоррекции текущего профиля.",
        confirmLabel: "Удалить слово",
      }))
    ) return;

    saving = true;
    dictionaryActionError = "";
    try {
      await invoke("delete_dictionary_word", { wordId: entry.id });
      await loadDashboard();
    } catch (cause) {
      dictionaryActionError = readableError(cause);
    } finally {
      saving = false;
    }
  }

  async function exportData(profileId: number | null) {
    transferBusy = true;
    transferError = "";
    transferSuccess = "";
    try {
      const result = await invoke<ExportResult | null>("export_configuration", { profileId });
      if (!result) return;
      transferSuccess = [
        profileId === null ? "Все профили экспортированы." : "Активный профиль экспортирован.",
        `${result.profileCount} профилей, ${result.phraseCount} фраз, ${result.wordCount} слов.`,
        `Файл: ${result.path}`,
      ].join(" ");
    } catch (cause) {
      transferError = String(cause);
    } finally {
      transferBusy = false;
    }
  }

  function chooseImportFile() {
    transferError = "";
    transferSuccess = "";
    importFileInput?.click();
  }

  async function previewImportFile(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file) return;
    if (file.size > 10 * 1024 * 1024) {
      transferError = "JSON-файл превышает допустимый размер 10 МБ.";
      return;
    }

    transferBusy = true;
    transferError = "";
    transferSuccess = "";
    try {
      const json = await file.text();
      const preview = await invoke<ImportPreview>("preview_import", { json });
      importFileName = file.name;
      importJson = json;
      importPreview = preview;
      importStrategy = preview.snippetConflicts + preview.wordConflicts > 0 ? "skip" : "overwrite";
      importModalOpen = true;
    } catch (cause) {
      transferError = String(cause);
    } finally {
      transferBusy = false;
    }
  }

  function closeImportModal() {
    importModalOpen = false;
    importFileName = "";
    importJson = "";
    importPreview = null;
    importStrategy = "skip";
  }

  async function applyImport() {
    if (!importJson || !importPreview) return;

    transferBusy = true;
    transferError = "";
    try {
      const result = await invoke<ImportResult>("import_configuration", {
        json: importJson,
        strategy: importStrategy,
      });
      const backup = result.backupPath ? ` Backup: ${result.backupPath}` : "";
      const skipped = result.skippedConflicts
        ? ` Пропущено конфликтов: ${result.skippedConflicts}.`
        : "";
      transferSuccess =
        `Импорт завершён: ${result.phraseCount} фраз, ${result.wordCount} слов.` +
        skipped +
        backup;
      closeImportModal();
      resetProfileWorkspace();
      await loadDashboard();
      await loadBackups();
      if (data?.activeProfileId) {
        await loadProfileSnapshots(data.activeProfileId);
      }
    } catch (cause) {
      transferError = String(cause);
    } finally {
      transferBusy = false;
    }
  }

  async function loadBackups() {
    if (!("__TAURI_INTERNALS__" in window)) return;

    backupsLoading = true;
    try {
      backups = await invoke<BackupSummary[]>("list_backups");
    } catch (cause) {
      transferError = String(cause);
    } finally {
      backupsLoading = false;
    }
  }

  async function loadProfileSnapshots(profileId: number | undefined = data?.activeProfileId) {
    if (!("__TAURI_INTERNALS__" in window) || !profileId) {
      profileSnapshots = [];
      return;
    }

    profileSnapshotsLoading = true;
    try {
      profileSnapshots = await invoke<ProfileSnapshotSummary[]>("list_profile_snapshots", {
        profileId,
      });
    } catch (cause) {
      transferError = String(cause);
    } finally {
      profileSnapshotsLoading = false;
    }
  }

  async function createActiveProfileSnapshot() {
    const profileId = data?.activeProfileId;
    if (!profileId) return;

    transferBusy = true;
    transferError = "";
    transferSuccess = "";
    try {
      const snapshot = await invoke<ProfileSnapshotSummary>("create_profile_snapshot", {
        profileId,
      });
      await loadProfileSnapshots(profileId);
      transferSuccess =
        `Снимок профиля «${snapshot.profileName}» создан: ` +
        `${formatBackupDate(snapshot.modifiedAtMs)}.`;
    } catch (cause) {
      transferError = String(cause);
    } finally {
      transferBusy = false;
    }
  }

  async function restoreProfileSnapshot(snapshot: ProfileSnapshotSummary) {
    const profileId = data?.activeProfileId;
    if (!profileId || snapshot.profileId !== profileId) return;

    const confirmed = await askForConfirmation({
      eyebrow: "Восстановление профиля",
      title: `Восстановить «${activeProfile?.name ?? snapshot.profileName}»?`,
      message:
        "Категории, фразы и словарь только этого профиля будут заменены данными снимка. Остальные профили не изменятся.",
      details: [
        formatBackupDate(snapshot.modifiedAtMs),
        formatFileSize(snapshot.sizeBytes),
        "Остальные профили не затрагиваются",
      ],
      confirmLabel: "Восстановить профиль",
    });
    if (!confirmed) return;

    transferBusy = true;
    transferError = "";
    transferSuccess = "";
    try {
      const result = await invoke<RestoreProfileSnapshotResult>("restore_profile_snapshot", {
        profileId,
        fileName: snapshot.fileName,
      });
      resetProfileWorkspace();
      await loadDashboard();
      await loadProfileSnapshots(profileId);
      transferSuccess =
        `Профиль восстановлен на ${formatBackupDate(snapshot.modifiedAtMs)}. ` +
        `Предыдущее состояние сохранено: ${result.safetySnapshotPath}`;
    } catch (cause) {
      transferError = String(cause);
    } finally {
      transferBusy = false;
    }
  }

  async function restoreDatabaseBackup(backup: BackupSummary) {
    const confirmed = await askForConfirmation({
      eyebrow: "Восстановление базы",
      title: "Вернуться к выбранной версии?",
      message:
        "Будет восстановлена вся база TextPilot: все профили, категории, фразы и словарь.",
      details: [
        "Это не откат одного профиля",
        formatBackupDate(backup.modifiedAtMs),
        formatFileSize(backup.sizeBytes),
      ],
      confirmLabel: "Восстановить версию",
    });
    if (!confirmed) return;

    transferBusy = true;
    transferError = "";
    transferSuccess = "";
    try {
      const result = await invoke<RestoreBackupResult>("restore_backup", {
        fileName: backup.fileName,
      });
      resetProfileWorkspace();
      await loadDashboard();
      await loadBackups();
      await loadProfileSnapshots();
      transferSuccess =
        `База восстановлена на ${formatBackupDate(backup.modifiedAtMs)}. ` +
        `Текущее состояние сохранено: ${result.safetyBackupPath}`;
    } catch (cause) {
      transferError = String(cause);
    } finally {
      transferBusy = false;
    }
  }

  function formatBackupDate(timestamp: number) {
    return new Intl.DateTimeFormat("ru-RU", {
      dateStyle: "medium",
      timeStyle: "short",
    }).format(new Date(timestamp));
  }

  function formatFileSize(bytes: number) {
    if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} КБ`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} МБ`;
  }

  function backupKind(fileName: string) {
    if (fileName.includes("before-restore")) return "До восстановления";
    if (fileName.includes("before-import")) return "До импорта";
    return "Резервная копия";
  }

  function profileSnapshotKind(fileName: string) {
    if (fileName.includes("-before-restore-")) return "До восстановления профиля";
    if (fileName.includes("-manual-")) return "Ручной снимок";
    return "Снимок профиля";
  }

  function openCategoryModal(
    parentId: number | null = null,
    category: CategorySummary | null = null,
  ) {
    editingCategoryId = category?.id ?? null;
    categoryName = category?.name ?? "";
    categoryParentId = String(category?.parentId ?? parentId ?? "");
    formError = "";
    categoryModalOpen = true;
  }

  function closeCategoryModal() {
    categoryModalOpen = false;
    editingCategoryId = null;
    categoryName = "";
    categoryParentId = "";
    formError = "";
  }

  async function saveCategory() {
    const name = categoryName.trim();
    formError = "";
    if (!name) {
      formError = "Введите название категории.";
      return;
    }
    if (name.includes("/")) {
      formError = "Название категории не может содержать символ /.";
      return;
    }

    saving = true;
    try {
      if (editingCategoryId === null) {
        await invoke("create_category", {
          input: {
            name,
            parentId: categoryParentId ? Number(categoryParentId) : null,
          },
        });
      } else {
        await invoke("rename_category", {
          input: { id: editingCategoryId, name },
        });
      }
      closeCategoryModal();
      await loadDashboard();
    } catch (cause) {
      formError = readableError(cause);
    } finally {
      saving = false;
    }
  }

  async function deleteCategory(category: CategorySummary | null) {
    if (!category) return;

    const descendants =
      data?.categories.filter((item) => item.path.startsWith(`${category.path}/`)).length ?? 0;
    const phraseCount = categorySubtreePhraseCount(category);
    const wordCount = categorySubtreeWordCount(category);
    const details = [
      descendants ? `${descendants} дочерних категорий` : null,
      phraseCount ? `${phraseCount} фраз перейдут в «Без категории»` : null,
      wordCount ? `${wordCount} слов перейдут в «Без категории»` : null,
    ]
      .filter((detail): detail is string => Boolean(detail));
    if (
      !(await askForConfirmation({
        eyebrow: "Удаление категории",
        title: `Удалить «${category.name}»?`,
        message:
          "Категория и её дочерние разделы будут удалены. Фразы и слова сохранятся без категории.",
        details,
        confirmLabel: "Удалить категорию",
      }))
    ) return;

    saving = true;
    categoryActionError = "";
    try {
      await invoke("delete_category", { categoryId: category.id });
      selectedCategory = "all";
      await loadDashboard();
    } catch (cause) {
      categoryActionError = readableError(cause);
    } finally {
      saving = false;
    }
  }

  async function savePhrase() {
    formError = "";
    const snippet = phraseSnippet.trim();
    if (!phraseTitle.trim() || !snippet || !phraseBody.trim()) {
      formError = "Заполните название, сниппет и текст фразы.";
      return;
    }
    if (snippet.length > 64 || !snippetPattern.test(snippet)) {
      formError = "Сниппет может содержать до 64 букв, цифр, дефисов и знаков подчёркивания.";
      return;
    }

    saving = true;
    try {
      const input = {
        title: phraseTitle,
        snippet,
        body: phraseBody,
        description: phraseDescription,
        categoryId: phraseCategoryId ? Number(phraseCategoryId) : null,
      };
      if (editingPhraseId === null) {
        await invoke("create_phrase", { input });
      } else {
        await invoke("update_phrase", {
          input: { id: editingPhraseId, ...input },
        });
      }
      closePhraseModal();
      await loadDashboard();
    } catch (cause) {
      formError = readableError(cause);
    } finally {
      saving = false;
    }
  }

  function openPhraseModal() {
    editingPhraseId = null;
    phraseTitle = "";
    phraseSnippet = "";
    phraseBody = "";
    phraseDescription = "";
    phraseCategoryId =
      typeof selectedCategory === "number" ? String(selectedCategory) : "";
    formError = "";
    phraseModalOpen = true;
  }

  function openEditPhraseModal(phrase: PhraseSummary | null) {
    if (!phrase) return;

    editingPhraseId = phrase.id;
    phraseTitle = phrase.title;
    phraseSnippet = phrase.snippet;
    phraseBody = phrase.body;
    phraseDescription = phrase.description;
    phraseCategoryId = phrase.categoryId === null ? "" : String(phrase.categoryId);
    formError = "";
    phraseActionError = "";
    phraseModalOpen = true;
  }

  function closePhraseModal() {
    phraseModalOpen = false;
    editingPhraseId = null;
    formError = "";
  }

  async function togglePhraseEnabled(phrase: PhraseSummary | null) {
    if (!phrase) return;

    saving = true;
    phraseActionError = "";
    try {
      await invoke("set_phrase_enabled", {
        phraseId: phrase.id,
        enabled: !phrase.isEnabled,
      });
      await loadDashboard();
    } catch (cause) {
      phraseActionError = readableError(cause);
    } finally {
      saving = false;
    }
  }

  async function deletePhrase(phrase: PhraseSummary | null) {
    if (!phrase) return;

    if (
      !(await askForConfirmation({
        eyebrow: "Удаление фразы",
        title: `Удалить «${phrase.title}»?`,
        message: `Сниппет «${phrase.snippet}» и текст фразы будут удалены без возможности восстановления.`,
        confirmLabel: "Удалить фразу",
      }))
    ) return;

    saving = true;
    phraseActionError = "";
    try {
      await invoke("delete_phrase", { phraseId: phrase.id });
      selectedPhrase = null;
      await loadDashboard();
    } catch (cause) {
      phraseActionError = readableError(cause);
    } finally {
      saving = false;
    }
  }

  function readableError(cause: unknown) {
    const message = String(cause);
    if (
      message.includes("UNIQUE constraint failed: phrases.profile_id, phrases.snippet") ||
      message.includes("UNIQUE constraint failed: phrases.profile_id, phrases.snippet_key")
    ) {
      return "Такой сниппет уже есть в активном профиле.";
    }
    if (message.includes("UNIQUE constraint failed: categories.profile_id, categories.path")) {
      return "Категория с таким названием уже существует.";
    }
    if (message.includes("UNIQUE constraint failed: profiles.name")) {
      return "Профиль с таким названием уже существует.";
    }
    if (
      message.includes("UNIQUE constraint failed: dictionary_words.profile_id, dictionary_words.word") ||
      message.includes("UNIQUE constraint failed: dictionary_words.profile_id, dictionary_words.word_key")
    ) {
      return "Такое слово уже есть в активном профиле.";
    }
    if (message.includes("cannot delete the last profile")) {
      return "Нельзя удалить единственный профиль.";
    }
    return "Не удалось сохранить изменения.";
  }

  function askForConfirmation(
    options: Omit<ConfirmationState, "resolve">,
  ): Promise<boolean> {
    confirmation?.resolve(false);
    return new Promise((resolve) => {
      confirmation = { ...options, resolve };
    });
  }

  function resolveConfirmation(confirmed: boolean) {
    const current = confirmation;
    if (!current) return;
    confirmation = null;
    current.resolve(confirmed);
  }

  function categoryNameById(categoryId: number | null) {
    if (categoryId === null) return "Без категории";
    return data?.categories.find((category) => category.id === categoryId)?.path ?? "Категория";
  }

  function categoryDepth(category: CategorySummary) {
    return Math.max(0, category.path.split("/").length - 1);
  }

  function resetCategoryDrag() {
    draggedCategoryId = null;
    categoryDropTargetId = null;
    categoryDropIndicatorId = null;
    categoryDropPosition = null;
    categoryPointerId = null;
    categoryPointerCategoryId = null;
    categoryPointerDragging = false;
  }

  function startCategoryPointer(event: PointerEvent, category: CategorySummary) {
    if (
      event.button !== 0 ||
      !event.isPrimary ||
      saving ||
      reorderingCategories
    ) {
      return;
    }

    categoryPointerId = event.pointerId;
    categoryPointerCategoryId = category.id;
    categoryPointerStartX = event.clientX;
    categoryPointerStartY = event.clientY;
    categoryPointerDragging = false;
    draggedCategoryId = null;
    categoryDropTargetId = null;
    categoryDropIndicatorId = null;
    categoryDropPosition = null;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  function updateCategoryDropTarget(clientX: number, clientY: number) {
    const targetElement = document
      .elementFromPoint(clientX, clientY)
      ?.closest<HTMLElement>("[data-category-id]");
    const targetId = Number(targetElement?.dataset.categoryId);
    const target = data?.categories.find((category) => category.id === targetId);
    const draggedCategory = data?.categories.find(
      (category) => category.id === draggedCategoryId,
    );
    if (
      !draggedCategory ||
      !targetElement ||
      !target ||
      draggedCategory.id === target.id ||
      draggedCategory.parentId !== target.parentId
    ) {
      categoryDropTargetId = null;
      categoryDropIndicatorId = null;
      categoryDropPosition = null;
      return;
    }

    const bounds = targetElement.getBoundingClientRect();
    const placeAfter = clientY >= bounds.top + bounds.height / 2;
    const subtree = data?.categories.filter(
      (category) =>
        category.path === target.path || category.path.startsWith(`${target.path}/`),
    );

    categoryDropTargetId = target.id;
    categoryDropPosition = placeAfter ? "after" : "before";
    categoryDropIndicatorId = placeAfter
      ? subtree?.at(-1)?.id ?? target.id
      : target.id;
  }

  function moveCategoryPointer(event: PointerEvent) {
    if (
      event.pointerId !== categoryPointerId ||
      categoryPointerCategoryId === null
    ) {
      return;
    }

    if (!categoryPointerDragging) {
      const distance = Math.hypot(
        event.clientX - categoryPointerStartX,
        event.clientY - categoryPointerStartY,
      );
      if (distance < 5) return;

      categoryPointerDragging = true;
      draggedCategoryId = categoryPointerCategoryId;
    }

    event.preventDefault();
    updateCategoryDropTarget(event.clientX, event.clientY);
  }

  async function finishCategoryPointer(event: PointerEvent) {
    if (event.pointerId !== categoryPointerId) return;

    const pointerTarget = event.currentTarget as HTMLElement;
    if (pointerTarget.hasPointerCapture(event.pointerId)) {
      pointerTarget.releasePointerCapture(event.pointerId);
    }

    const wasDragging = categoryPointerDragging;
    const categoryId = draggedCategoryId;
    const targetCategoryId = categoryDropTargetId;
    const placeAfter = categoryDropPosition === "after";
    resetCategoryDrag();

    if (!wasDragging) return;

    event.preventDefault();
    suppressCategoryClick = true;
    window.setTimeout(() => {
      suppressCategoryClick = false;
    });

    if (
      categoryId === null ||
      targetCategoryId === null
    ) {
      return;
    }

    reorderingCategories = true;
    categoryActionError = "";
    try {
      await invoke("reorder_category", {
        input: { categoryId, targetCategoryId, placeAfter },
      });
      await loadDashboard();
    } catch (cause) {
      categoryActionError = readableError(cause);
    } finally {
      reorderingCategories = false;
    }
  }

  function cancelCategoryPointer(event: PointerEvent) {
    if (event.pointerId !== categoryPointerId) return;
    resetCategoryDrag();
  }

  function selectPhraseCategory(categoryId: number) {
    if (suppressCategoryClick) return;
    selectedCategory = categoryId;
  }

  function selectDictionaryCategory(categoryId: number) {
    if (suppressCategoryClick) return;
    selectedDictionaryCategory = categoryId;
  }

  function categorySubtreePhraseCount(category: CategorySummary) {
    return (
      data?.categories
        .filter(
          (item) =>
            item.path === category.path || item.path.startsWith(`${category.path}/`),
        )
        .reduce((total, item) => total + item.phraseCount, 0) ?? 0
    );
  }

  function categorySubtreeWordCount(category: CategorySummary) {
    return (
      data?.categories
        .filter(
          (item) =>
            item.path === category.path || item.path.startsWith(`${category.path}/`),
        )
        .reduce((total, item) => total + item.wordCount, 0) ?? 0
    );
  }
</script>

<svelte:head>
  <title>TextPilot</title>
</svelte:head>

<svelte:window
  onclick={() => (profileDropdownOpen = false)}
  onkeydown={(event) => {
    if (event.key === "Escape") profileDropdownOpen = false;
  }}
/>

<div class="app-shell" class:is-paused={paused}>
  <aside class="primary-sidebar">
    <div class="brand" aria-label="TextPilot">
      <span class="brand-mark"><Icon name="spark" size={19} strokeWidth={2.1} /></span>
      <span>TextPilot</span>
    </div>

    <nav class="primary-nav" aria-label="Основная навигация">
      {#each navItems as item}
        <button
          class:active={activeSection === item.id}
          class="nav-item"
          onclick={() => {
            activeSection = item.id;
            if (item.id === "settings") {
              void loadBackups();
              void loadProfileSnapshots();
            }
          }}
          title={item.label}
        >
          <Icon name={item.icon} size={20} />
          <span>{item.label}</span>
          {#if item.id === "dictionary" && data?.dictionaryCount}
            <span class="nav-count">{data.dictionaryCount}</span>
          {/if}
        </button>
      {/each}
    </nav>

    <div class="privacy-note">
      <Icon name="shield" size={18} />
      <div>
        <strong>Только локально</strong>
        <span>Текст не покидает компьютер</span>
      </div>
    </div>
  </aside>

  <section class="workspace">
    <header class="topbar">
      <div class="profile-switcher-wrap">
        <button
          class:open={profileDropdownOpen}
          class="profile-switcher"
          onclick={(event) => {
            event.stopPropagation();
            profileDropdownOpen = !profileDropdownOpen;
          }}
          aria-haspopup="menu"
          aria-expanded={profileDropdownOpen}
        >
          <span class="profile-avatar">{activeProfile?.name.slice(0, 1) ?? "О"}</span>
          <span>
            <small>Активный профиль</small>
            <strong>{activeProfile?.name ?? "Основной"}</strong>
          </span>
          <Icon name="chevron" size={15} />
        </button>

        {#if profileDropdownOpen}
          <div class="profile-dropdown">
            <div class="profile-dropdown-heading">
              <span>Переключить профиль</span>
              <small>{data?.profiles.length ?? 0}</small>
            </div>
            <div class="profile-dropdown-list">
              {#each data?.profiles ?? [] as profile}
                <button
                  class:active={profile.isActive}
                  class="profile-dropdown-item"
                  onclick={() => activateProfile(profile)}
                  disabled={saving}
                >
                  <span class="profile-avatar small">
                    {profile.name.slice(0, 1).toLocaleUpperCase("ru")}
                  </span>
                  <span class="profile-dropdown-copy">
                    <strong>{profile.name}</strong>
                    <small>{profile.phraseCount} фраз</small>
                  </span>
                  {#if profile.isActive}
                    <span class="profile-dropdown-check"><Icon name="check" size={15} /></span>
                  {/if}
                </button>
              {/each}
            </div>
            <button class="profile-dropdown-manage" onclick={openProfileManagement}>
              <Icon name="settings" size={15} />
              Управление профилями
            </button>
          </div>
        {/if}
      </div>

      <div class="topbar-actions">
        <div class="engine-status">
          <span class:paused class:unavailable={!data?.engineAvailable} class="status-dot"></span>
          {data?.engineAvailable === false ? "Движок недоступен" : paused ? "Приостановлен" : "Работает"}
        </div>
        <button
          class:paused
          class="pause-button"
          onclick={togglePaused}
          disabled={data?.engineAvailable === false}
          title={paused ? "Продолжить работу" : "Приостановить работу"}
        >
          <Icon name={paused ? "play" : "pause"} size={17} />
          {paused ? "Продолжить" : "Пауза"}
        </button>
      </div>
    </header>

    {#if activeSection === "phrases"}
      <div class="content-layout">
        <aside class="category-sidebar">
          <div class="sidebar-heading">
            <div>
              <span>Библиотека</span>
              <strong>Категории</strong>
            </div>
            <button
              class="icon-button"
              onclick={() => openCategoryModal()}
              title="Добавить категорию"
            >
              <Icon name="plus" size={17} />
            </button>
          </div>

          <div class:dragging-categories={categoryPointerDragging} class="category-list">
            <button
              class:active={selectedCategory === "all"}
              onclick={() => (selectedCategory = "all")}
            >
              <span class="category-icon all"><Icon name="spark" size={15} /></span>
              <span>Все фразы</span>
              <small>{data?.phrases.length ?? 0}</small>
            </button>

            {#each data?.categories ?? [] as category (category.id)}
              <button
                class:active={selectedCategory === category.id}
                class:dragging={draggedCategoryId === category.id}
                class:drop-before={categoryDropIndicatorId === category.id && categoryDropPosition === "before"}
                class:drop-after={categoryDropIndicatorId === category.id && categoryDropPosition === "after"}
                class="tree-category"
                style={`padding-left: ${9 + categoryDepth(category) * 15}px`}
                data-category-id={category.id}
                onclick={() => selectPhraseCategory(category.id)}
                onpointerdown={(event) => startCategoryPointer(event, category)}
                onpointermove={moveCategoryPointer}
                onpointerup={finishCategoryPointer}
                onpointercancel={cancelCategoryPointer}
                title={category.path}
              >
                <span class="category-icon"><Icon name="folder" size={15} /></span>
                <span>{category.name}</span>
                <small>{categorySubtreePhraseCount(category)}</small>
              </button>
            {/each}

            <button
              class:active={selectedCategory === "uncategorized"}
              onclick={() => (selectedCategory = "uncategorized")}
            >
              <span class="category-icon muted"><Icon name="folder" size={15} /></span>
              <span>Без категории</span>
              <small>{data?.phrases.filter((phrase) => phrase.categoryId === null).length ?? 0}</small>
            </button>
          </div>

          {#if selectedCategoryRecord}
            <div class="category-tools">
              <div>
                <span>Выбрана категория</span>
                <strong title={selectedCategoryRecord.path}>{selectedCategoryRecord.name}</strong>
              </div>
              <div class="category-tool-actions">
                <button
                  onclick={() => openCategoryModal(selectedCategoryRecord?.id ?? null)}
                  title="Добавить вложенную категорию"
                  aria-label="Добавить вложенную категорию"
                  disabled={saving}
                >
                  <Icon name="plus" size={15} />
                </button>
                <button
                  onclick={() => openCategoryModal(null, selectedCategoryRecord)}
                  title="Переименовать категорию"
                  aria-label="Переименовать категорию"
                  disabled={saving}
                >
                  <Icon name="edit" size={15} />
                </button>
                <button
                  class="danger"
                  onclick={() => deleteCategory(selectedCategoryRecord)}
                  title="Удалить категорию"
                  aria-label="Удалить категорию"
                  disabled={saving}
                >
                  <Icon name="trash" size={15} />
                </button>
              </div>
              {#if categoryActionError}
                <p class="inline-error">{categoryActionError}</p>
              {/if}
            </div>
          {/if}

          <div class="shortcut-card">
            <kbd>Alt</kbd><span>+</span><kbd>Space</kbd>
            <p>Быстрый поиск фраз</p>
          </div>
        </aside>

        <main class="main-content">
          <div class="page-header">
            <div>
              <span class="page-kicker">Рабочие шаблоны</span>
              <h1>Фразы и сниппеты</h1>
              <p>Введите короткую команду и нажмите Tab, чтобы вставить готовый текст.</p>
            </div>
            <button class="primary-button" onclick={openPhraseModal}>
              <Icon name="plus" size={18} strokeWidth={2.2} />
              Новая фраза
            </button>
          </div>

          <div class="toolbar">
            <label class="search-field">
              <Icon name="search" size={18} />
              <input bind:value={search} placeholder="Поиск по названию, сниппету или тексту" />
              {#if search}
                <button onclick={() => (search = "")} aria-label="Очистить поиск">
                  <Icon name="close" size={15} />
                </button>
              {/if}
            </label>
            <span class="result-count">{filteredPhrases.length} {filteredPhrases.length === 1 ? "фраза" : "фраз"}</span>
          </div>

          {#if loading}
            <div class="loading-state">
              <span></span><span></span><span></span>
            </div>
          {:else if error}
            <div class="message-state error-state">
              <strong>Не удалось загрузить библиотеку</strong>
              <p>{error}</p>
              <button class="secondary-button" onclick={loadDashboard}>Повторить</button>
            </div>
          {:else if filteredPhrases.length === 0}
            <div class="message-state empty-state">
              <div class="empty-icon"><Icon name="phrase" size={26} /></div>
              <strong>{search ? "Ничего не найдено" : "Здесь появятся ваши фразы"}</strong>
              <p>
                {search
                  ? "Попробуйте изменить запрос или выбрать другую категорию."
                  : "Создайте первую фразу, затем введите её сниппет, например кп, и нажмите Tab."}
              </p>
              {#if !search}
                <button class="secondary-button" onclick={openPhraseModal}>
                  <Icon name="plus" size={17} />
                  Создать фразу
                </button>
              {/if}
            </div>
          {:else}
            <div class="phrase-list">
              {#each filteredPhrases as phrase}
                <button
                  class:selected={selectedPhrase?.id === phrase.id}
                  class="phrase-card"
                  onclick={() => (selectedPhrase = phrase)}
                >
                  <span class="phrase-glyph">{phrase.title.slice(0, 1).toLocaleUpperCase("ru")}</span>
                  <span class="phrase-content">
                    <span class="phrase-heading">
                      <strong>{phrase.title}</strong>
                      <code>{phrase.snippet}</code>
                    </span>
                    <span class="phrase-preview">{phrase.body}</span>
                    <span class="phrase-meta">
                      <span>{categoryNameById(phrase.categoryId)}</span>
                      <span class:enabled={phrase.isEnabled} class="enabled-label">
                        {phrase.isEnabled ? "Активна" : "Выключена"}
                      </span>
                    </span>
                  </span>
                  <span class="card-menu"><Icon name="more" size={18} /></span>
                </button>
              {/each}
            </div>
          {/if}
        </main>

        {#if selectedPhrase}
          <aside class="detail-panel">
            <div class="detail-header">
              <span>Просмотр фразы</span>
              <button class="icon-button" onclick={() => (selectedPhrase = null)} title="Закрыть">
                <Icon name="close" size={18} />
              </button>
            </div>
            <div class="detail-body">
              <div class="detail-title">
                <span class="phrase-glyph large">{selectedPhrase.title.slice(0, 1)}</span>
                <div>
                  <h2>{selectedPhrase.title}</h2>
                  <code>{selectedPhrase.snippet}</code>
                </div>
              </div>
              <div class="detail-actions">
                <button
                  class="secondary-button"
                  onclick={() => openEditPhraseModal(selectedPhrase)}
                  disabled={saving}
                >
                  Редактировать
                </button>
                <button
                  class="ghost-button"
                  onclick={() => togglePhraseEnabled(selectedPhrase)}
                  disabled={saving}
                >
                  {selectedPhrase.isEnabled ? "Выключить" : "Включить"}
                </button>
                <button
                  class="danger-button"
                  onclick={() => deletePhrase(selectedPhrase)}
                  disabled={saving}
                >
                  Удалить
                </button>
              </div>
              {#if phraseActionError}
                <p class="detail-error">{phraseActionError}</p>
              {/if}
              <div class="detail-section">
                <span class="detail-label">Категория</span>
                <p>{categoryNameById(selectedPhrase.categoryId)}</p>
              </div>
              <div class="detail-section">
                <span class="detail-label">Текст фразы</span>
                <div class="phrase-text">{selectedPhrase.body}</div>
              </div>
              {#if selectedPhrase.description}
                <div class="detail-section">
                  <span class="detail-label">Описание</span>
                  <p>{selectedPhrase.description}</p>
                </div>
              {/if}
            </div>
          </aside>
        {/if}
      </div>
    {:else if activeSection === "dictionary"}
      <div class="dictionary-layout">
        <aside class="category-sidebar">
          <div class="sidebar-heading">
            <div>
              <span>Словарь</span>
              <strong>Категории</strong>
            </div>
          </div>

          <div class:dragging-categories={categoryPointerDragging} class="category-list">
            <button
              class:active={selectedDictionaryCategory === "all"}
              onclick={() => (selectedDictionaryCategory = "all")}
            >
              <span class="category-icon all"><Icon name="book" size={15} /></span>
              <span>Все слова</span>
              <small>{data?.dictionaryWords.length ?? 0}</small>
            </button>

            {#each data?.categories ?? [] as category (category.id)}
              <button
                class:active={selectedDictionaryCategory === category.id}
                class:dragging={draggedCategoryId === category.id}
                class:drop-before={categoryDropIndicatorId === category.id && categoryDropPosition === "before"}
                class:drop-after={categoryDropIndicatorId === category.id && categoryDropPosition === "after"}
                class="tree-category"
                style={`padding-left: ${9 + categoryDepth(category) * 15}px`}
                data-category-id={category.id}
                onclick={() => selectDictionaryCategory(category.id)}
                onpointerdown={(event) => startCategoryPointer(event, category)}
                onpointermove={moveCategoryPointer}
                onpointerup={finishCategoryPointer}
                onpointercancel={cancelCategoryPointer}
                title={category.path}
              >
                <span class="category-icon"><Icon name="folder" size={15} /></span>
                <span>{category.name}</span>
                <small>{categorySubtreeWordCount(category)}</small>
              </button>
            {/each}

            <button
              class:active={selectedDictionaryCategory === "uncategorized"}
              onclick={() => (selectedDictionaryCategory = "uncategorized")}
            >
              <span class="category-icon muted"><Icon name="folder" size={15} /></span>
              <span>Без категории</span>
              <small>
                {data?.dictionaryWords.filter((entry) => entry.categoryId === null).length ?? 0}
              </small>
            </button>
          </div>

          <div class="dictionary-note">
            <Icon name="shield" size={16} />
            <p>Добавляйте только правильные слова. Ошибочные варианты программа определит сама.</p>
          </div>
        </aside>

        <main class="main-content dictionary-page">
          <div class="page-header">
            <div>
              <span class="page-kicker">Правильные слова</span>
              <h1>Пользовательский словарь</h1>
              <p>Слова из активного профиля будут использоваться для подсказок и исправлений.</p>
            </div>
            <button class="primary-button" onclick={() => openDictionaryModal()}>
              <Icon name="plus" size={18} strokeWidth={2.2} />
              Добавить слово
            </button>
          </div>

          <div class="toolbar">
            <label class="search-field">
              <Icon name="search" size={18} />
              <input bind:value={dictionarySearch} placeholder="Поиск по словарю" />
              {#if dictionarySearch}
                <button onclick={() => (dictionarySearch = "")} aria-label="Очистить поиск">
                  <Icon name="close" size={15} />
                </button>
              {/if}
            </label>
            <span class="result-count">
              {filteredDictionaryWords.length} слов
            </span>
          </div>

          {#if dictionaryActionError}
            <p class="page-error">{dictionaryActionError}</p>
          {/if}

          {#if filteredDictionaryWords.length === 0}
            <div class="message-state empty-state">
              <div class="empty-icon"><Icon name="book" size={26} /></div>
              <strong>{dictionarySearch ? "Слово не найдено" : "Словарь пока пуст"}</strong>
              <p>
                {dictionarySearch
                  ? "Измените запрос или выберите другую категорию."
                  : "Добавьте правильные рабочие слова для будущего автокомплита и автокоррекции."}
              </p>
              {#if !dictionarySearch}
                <button class="secondary-button" onclick={() => openDictionaryModal()}>
                  <Icon name="plus" size={17} />
                  Добавить слово
                </button>
              {/if}
            </div>
          {:else}
            <div class="dictionary-list">
              {#each filteredDictionaryWords as entry}
                <article class:disabled={!entry.isEnabled} class="dictionary-card">
                  <span class="dictionary-glyph">
                    {entry.word.slice(0, 1).toLocaleUpperCase("ru")}
                  </span>

                  <div class="dictionary-content">
                    <div class="dictionary-heading">
                      <strong>{entry.word}</strong>
                      <span class="dictionary-priority-chip" title="Приоритет слова">
                        П {entry.priority}
                      </span>
                    </div>

                    <div class="dictionary-meta">
                      <span>{categoryNameById(entry.categoryId)}</span>
                      <button
                        class:enabled={entry.isEnabled}
                        onclick={() =>
                          updateDictionaryWord(entry, { isEnabled: !entry.isEnabled })}
                        disabled={saving}
                        title="Включить или отключить слово"
                      >
                        {entry.isEnabled ? "Активно" : "Выключено"}
                      </button>
                      <button
                        class:enabled={entry.autocompleteEnabled}
                        onclick={() =>
                          updateDictionaryWord(entry, {
                            autocompleteEnabled: !entry.autocompleteEnabled,
                          })}
                        disabled={saving}
                        title="Использовать для автокомплита"
                      >
                        Автокомплит
                      </button>
                      <button
                        class:enabled={entry.autocorrectEnabled}
                        onclick={() =>
                          updateDictionaryWord(entry, {
                            autocorrectEnabled: !entry.autocorrectEnabled,
                          })}
                        disabled={saving}
                        title="Использовать для автокоррекции"
                      >
                        Автокоррекция
                      </button>
                    </div>
                  </div>

                  <div class="dictionary-actions">
                    <details class="dictionary-menu">
                      <summary title="Действия со словом" aria-label="Действия со словом">
                        <Icon name="more" size={17} strokeWidth={2} />
                      </summary>
                      <div class="dictionary-menu-panel">
                        <button
                          type="button"
                          onclick={() => openDictionaryModal(entry)}
                          disabled={saving}
                        >
                          <Icon name="edit" size={15} />
                          Редактировать
                        </button>
                        <button
                          type="button"
                          class="danger-action"
                          onclick={() => deleteDictionaryWord(entry)}
                          disabled={saving}
                        >
                          <Icon name="trash" size={15} />
                          Удалить
                        </button>
                      </div>
                    </details>
                  </div>
                </article>
              {/each}
            </div>
          {/if}
        </main>
      </div>
    {:else if activeSection === "profiles"}
      <main class="main-content profiles-page">
        <div class="page-header">
          <div>
            <span class="page-kicker">Рабочие пространства</span>
            <h1>Профили</h1>
            <p>Каждый профиль хранит собственные категории, фразы, сниппеты и словарь.</p>
          </div>
          <button class="primary-button" onclick={() => openProfileModal()}>
            <Icon name="plus" size={18} strokeWidth={2.2} />
            Новый профиль
          </button>
        </div>

        {#if profileActionError}
          <p class="page-error">{profileActionError}</p>
        {/if}

        <div class="profile-grid">
          {#each data?.profiles ?? [] as profile}
            <article class:active={profile.isActive} class="profile-card">
              <div class="profile-card-heading">
                <span class="profile-avatar large">
                  {profile.name.slice(0, 1).toLocaleUpperCase("ru")}
                </span>
                <div>
                  <h2>{profile.name}</h2>
                  <span class:active={profile.isActive} class="profile-state">
                    {profile.isActive ? "Активный профиль" : "Неактивен"}
                  </span>
                </div>
              </div>

              <div class="profile-stats">
                <span><strong>{profile.phraseCount}</strong> фраз</span>
                <span><strong>{profile.categoryCount}</strong> категорий</span>
                <span><strong>{profile.dictionaryCount}</strong> слов</span>
              </div>

              <div class="profile-actions">
                <button
                  class={profile.isActive ? "ghost-button" : "secondary-button"}
                  onclick={() => activateProfile(profile)}
                  disabled={saving || profile.isActive}
                >
                  {profile.isActive ? "Сейчас активен" : "Сделать активным"}
                </button>
                <button
                  class="ghost-button"
                  onclick={() => openProfileModal(profile)}
                  disabled={saving}
                >
                  Переименовать
                </button>
                <button
                  class="danger-button compact"
                  onclick={() => deleteProfile(profile)}
                  disabled={saving || (data?.profiles.length ?? 0) <= 1}
                  title={(data?.profiles.length ?? 0) <= 1
                    ? "В системе должен остаться хотя бы один профиль"
                    : "Удалить профиль"}
                >
                  Удалить
                </button>
              </div>
            </article>
          {/each}
        </div>
      </main>
    {:else if activeSection === "settings"}
      <main class="main-content settings-page">
        <div class="page-header">
          <div>
            <span class="page-kicker">Конфигурация</span>
            <h1>Настройки</h1>
            <p>
              Управляйте горячими клавишами и переносите профили через локальные JSON-файлы.
              Все настройки хранятся только на этом компьютере.
            </p>
          </div>
        </div>

        {#if transferError}
          <p class="page-error transfer-message">{transferError}</p>
        {/if}
        {#if transferSuccess}
          <p class="page-success transfer-message">{transferSuccess}</p>
        {/if}

        <div class="settings-grid">
          <section class="settings-card hotkey-settings-card">
            <div class="settings-card-icon"><Icon name="book" size={21} /></div>
            <div class="settings-card-copy">
              <span class="page-kicker">Производительность</span>
              <h2>Подсказки из словаря</h2>
              <p>
                Ищет варианты в пользовательском словаре после каждого введённого символа.
                Отключите на слабых компьютерах — сниппеты и автокоррекция продолжат работать.
              </p>
            </div>
            <div class="hotkey-setting-row">
              <span class="hotkey-setting-copy dictionary-setting-copy">
                <strong>Подсказки при вводе</strong>
                <small>{data?.dictionaryAutocompleteEnabled ? "Включены" : "Выключены"}</small>
              </span>
              <button
                type="button"
                class:enabled={data?.dictionaryAutocompleteEnabled ?? true}
                class="setting-switch"
                role="switch"
                aria-checked={data?.dictionaryAutocompleteEnabled ?? true}
                aria-label="Подсказки из пользовательского словаря"
                onclick={toggleDictionaryAutocomplete}
                disabled={dictionaryAutocompleteSaving || !data}
              >
                <span></span>
              </button>
            </div>
            {#if dictionaryAutocompleteError}
              <p class="page-error hotkey-setting-error">{dictionaryAutocompleteError}</p>
            {/if}
          </section>

          <section class="settings-card hotkey-settings-card">
            <div class="settings-card-icon"><Icon name="search" size={21} /></div>
            <div class="settings-card-copy">
              <span class="page-kicker">Горячие клавиши</span>
              <h2>Быстрый поиск</h2>
              <p>
                Открывает поиск по фразам и словарю из любого приложения. Когда функция
                выключена, сочетание остаётся доступным Windows и другим программам.
              </p>
            </div>
            <div class="hotkey-setting-row">
              <span class="hotkey-setting-copy">
                <kbd>Alt</kbd><span>+</span><kbd>Space</kbd>
                <small>{data?.quickSearchEnabled ? "Включено" : "Выключено"}</small>
              </span>
              <button
                type="button"
                class:enabled={data?.quickSearchEnabled ?? true}
                class="setting-switch"
                role="switch"
                aria-checked={data?.quickSearchEnabled ?? true}
                aria-label="Быстрый поиск по Alt + Space"
                onclick={toggleQuickSearch}
                disabled={quickSearchSaving || !data}
              >
                <span></span>
              </button>
            </div>
            {#if quickSearchError}
              <p class="page-error hotkey-setting-error">{quickSearchError}</p>
            {/if}
          </section>

          <section class="settings-card">
            <div class="settings-card-icon"><Icon name="download" size={21} /></div>
            <div class="settings-card-copy">
              <span class="page-kicker">Резервная копия конфигурации</span>
              <h2>Экспорт JSON</h2>
              <p>
                Файл содержит категории, фразы, сниппеты, словарь и рабочие флаги.
                Перед сохранением можно выбрать папку и изменить имя файла.
              </p>
            </div>
            <div class="settings-actions">
              <button
                class="secondary-button"
                onclick={() => exportData(data?.activeProfileId ?? null)}
                disabled={transferBusy || !data?.activeProfileId}
              >
                <Icon name="download" size={16} />
                Активный профиль
              </button>
              <button
                class="primary-button"
                onclick={() => exportData(null)}
                disabled={transferBusy}
              >
                <Icon name="download" size={16} />
                Все профили
              </button>
            </div>
          </section>

          <section class="settings-card">
            <div class="settings-card-icon"><Icon name="upload" size={21} /></div>
            <div class="settings-card-copy">
              <span class="page-kicker">Восстановление и перенос</span>
              <h2>Импорт JSON</h2>
              <p>
                Сначала увидите состав файла и количество конфликтов. Данные меняются
                только после подтверждения стратегии.
              </p>
            </div>
            <div class="settings-actions">
              <button
                class="primary-button"
                onclick={chooseImportFile}
                disabled={transferBusy}
              >
                <Icon name="upload" size={16} />
                Выбрать JSON-файл
              </button>
            </div>
          </section>

          <section class="settings-card backup-history-card">
            <div class="settings-card-icon"><Icon name="history" size={21} /></div>
            <div class="settings-card-copy">
              <span class="page-kicker">История активного профиля</span>
              <h2>{activeProfile?.name ?? "Активный профиль"}</h2>
              <p>
                Снимок содержит категории, фразы и словарь только этого профиля.
                Восстановление не меняет остальные профили.
              </p>
            </div>
            <div class="settings-actions">
              <button
                class="primary-button"
                onclick={createActiveProfileSnapshot}
                disabled={transferBusy || !data?.activeProfileId}
              >
                <Icon name="plus" size={16} />
                Создать снимок профиля
              </button>
            </div>

            <div class="backup-list">
              {#if profileSnapshotsLoading}
                <div class="backup-empty">Загружаю снимки профиля...</div>
              {:else if profileSnapshots.length === 0}
                <div class="backup-empty">У этого профиля пока нет снимков.</div>
              {:else}
                {#each profileSnapshots as snapshot}
                  <div class="backup-row">
                    <span class="backup-row-icon"><Icon name="history" size={16} /></span>
                    <span class="backup-row-copy">
                      <strong>{formatBackupDate(snapshot.modifiedAtMs)}</strong>
                      <small>
                        {profileSnapshotKind(snapshot.fileName)} ·
                        {formatFileSize(snapshot.sizeBytes)}
                      </small>
                    </span>
                    <button
                      type="button"
                      class="secondary-button"
                      onclick={() => restoreProfileSnapshot(snapshot)}
                      disabled={transferBusy}
                    >
                      Восстановить профиль
                    </button>
                  </div>
                {/each}
              {/if}
            </div>
          </section>

          <section class="settings-card backup-history-card">
            <div class="settings-card-icon"><Icon name="history" size={21} /></div>
            <div class="settings-card-copy">
              <span class="page-kicker">История базы данных</span>
              <h2>Предыдущие версии</h2>
              <p>
                Это глобальные снимки всей базы: восстановление затрагивает все профили.
                Перед откатом TextPilot сохранит текущее состояние отдельным снимком.
              </p>
            </div>

            <div class="backup-list">
              {#if backupsLoading}
                <div class="backup-empty">Загружаю резервные копии...</div>
              {:else if backups.length === 0}
                <div class="backup-empty">
                  Резервные копии появятся после первого импорта или восстановления.
                </div>
              {:else}
                {#each backups as backup}
                  <div class="backup-row">
                    <span class="backup-row-icon"><Icon name="history" size={16} /></span>
                    <span class="backup-row-copy">
                      <strong>{formatBackupDate(backup.modifiedAtMs)}</strong>
                      <small>{backupKind(backup.fileName)} · {formatFileSize(backup.sizeBytes)}</small>
                    </span>
                    <button
                      type="button"
                      class="secondary-button"
                      onclick={() => restoreDatabaseBackup(backup)}
                      disabled={transferBusy}
                    >
                      Восстановить
                    </button>
                  </div>
                {/each}
              {/if}
            </div>
          </section>

          <section class="settings-card safety-card">
            <div class="settings-card-icon"><Icon name="shield" size={21} /></div>
            <div class="settings-card-copy">
              <span class="page-kicker">Безопасность данных</span>
              <h2>Как работает импорт</h2>
              <p>
                JSON валидируется до записи. Изменения применяются одной транзакцией,
                а backup SQLite хранится рядом с базой в папке `backups`.
              </p>
            </div>
          </section>
        </div>

        <input
          class="visually-hidden"
          bind:this={importFileInput}
          type="file"
          accept=".json,application/json"
          onchange={previewImportFile}
        />
      </main>
    {/if}
  </section>
</div>

{#if phraseModalOpen}
  <div class="modal-backdrop" role="presentation" onclick={(event) => event.target === event.currentTarget && closePhraseModal()}>
    <div class="modal" role="dialog" aria-modal="true" aria-labelledby="new-phrase-title">
      <header class="modal-header">
        <div>
          <span class="page-kicker">{editingPhraseId === null ? "Новый шаблон" : "Изменение шаблона"}</span>
          <h2 id="new-phrase-title">{editingPhraseId === null ? "Создать фразу" : "Редактировать фразу"}</h2>
        </div>
        <button class="icon-button" onclick={closePhraseModal} title="Закрыть">
          <Icon name="close" size={19} />
        </button>
      </header>

      <form onsubmit={(event) => { event.preventDefault(); savePhrase(); }}>
        <div class="form-grid">
          <label class="form-field wide">
            <span>Название</span>
            <input bind:value={phraseTitle} placeholder="Коммерческое предложение" />
          </label>

          <label class="form-field">
            <span>Сниппет</span>
            <div class="snippet-input">
              <input
                bind:value={phraseSnippet}
                placeholder="кп"
                maxlength="64"
                spellcheck="false"
                autocomplete="off"
              />
            </div>
            <small>Введите сниппет и нажмите Tab для вставки фразы.</small>
          </label>

          <div class="form-field">
            <span>Категория</span>
            <SelectControl
              bind:value={phraseCategoryId}
              ariaLabel="Категория фразы"
              options={[
                { value: "", label: "Без категории" },
                ...(data?.categories ?? []).map((category) => ({
                  value: String(category.id),
                  label: category.path,
                })),
              ]}
            />
          </div>

          <label class="form-field wide">
            <span>Текст фразы</span>
            <textarea
              bind:value={phraseBody}
              rows="7"
              placeholder="Здравствуйте! Отправляю вам коммерческое предложение..."
            ></textarea>
            <small>Переносы и пустые строки сохраняются.</small>
          </label>

          <label class="form-field wide">
            <span>Описание <em>необязательно</em></span>
            <input bind:value={phraseDescription} placeholder="Когда использовать эту фразу" />
          </label>
        </div>

        {#if formError}<p class="form-error">{formError}</p>{/if}

        <footer class="modal-footer">
          <button type="button" class="ghost-button" onclick={closePhraseModal}>Отмена</button>
          <button type="submit" class="primary-button" disabled={saving}>
            {saving
              ? "Сохраняем..."
              : editingPhraseId === null
                ? "Создать фразу"
                : "Сохранить изменения"}
          </button>
        </footer>
      </form>
    </div>
  </div>
{/if}

{#if importModalOpen && importPreview}
  <div
    class="modal-backdrop"
    role="presentation"
    onclick={(event) => event.target === event.currentTarget && closeImportModal()}
  >
    <div class="modal import-modal" role="dialog" aria-modal="true" aria-labelledby="import-modal-title">
      <header class="modal-header">
        <div>
          <span class="page-kicker">Предварительный просмотр</span>
          <h2 id="import-modal-title">Импорт «{importFileName}»</h2>
        </div>
        <button class="icon-button" onclick={closeImportModal} title="Закрыть">
          <Icon name="close" size={19} />
        </button>
      </header>

      <div class="import-summary">
        <span><strong>{importPreview.profileCount}</strong> профилей</span>
        <span><strong>{importPreview.categoryCount}</strong> категорий</span>
        <span><strong>{importPreview.phraseCount}</strong> фраз</span>
        <span><strong>{importPreview.wordCount}</strong> слов</span>
      </div>

      {#if importPreview.targetProfileName}
        <p class="import-target">
          Данные будут импортированы в активный профиль
          <strong>«{importPreview.targetProfileName}»</strong>.
        </p>
      {/if}

      <div class="conflict-summary" class:clean={importPreview.snippetConflicts + importPreview.wordConflicts === 0}>
        <Icon
          name={importPreview.snippetConflicts + importPreview.wordConflicts === 0
            ? "check"
            : "shield"}
          size={18}
        />
        <div>
          <strong>
            {importPreview.snippetConflicts + importPreview.wordConflicts === 0
              ? "Конфликтов не найдено"
              : "Найдены совпадения"}
          </strong>
          <span>
            Сниппеты: {importPreview.snippetConflicts}, слова: {importPreview.wordConflicts}
          </span>
        </div>
      </div>

      {#if importPreview.snippetConflicts + importPreview.wordConflicts > 0}
        <fieldset class="strategy-list">
          <legend>Что делать с конфликтами</legend>
          <label class:active={importStrategy === "skip"}>
            <input type="radio" bind:group={importStrategy} value="skip" />
            <span>
              <strong>Пропустить конфликты</strong>
              <small>Существующие фразы и слова останутся без изменений.</small>
            </span>
          </label>
          <label class:active={importStrategy === "overwrite"}>
            <input type="radio" bind:group={importStrategy} value="overwrite" />
            <span>
              <strong>Перезаписать конфликты</strong>
              <small>Совпавшие сниппеты и слова будут заменены данными из файла.</small>
            </span>
          </label>
        </fieldset>
      {/if}

      {#if transferError}<p class="form-error">{transferError}</p>{/if}

      <footer class="modal-footer">
        <button type="button" class="ghost-button" onclick={closeImportModal}>Отмена</button>
        <button type="button" class="primary-button" onclick={applyImport} disabled={transferBusy}>
          {transferBusy ? "Импортируем..." : "Создать backup и импортировать"}
        </button>
      </footer>
    </div>
  </div>
{/if}

{#if dictionaryModalOpen}
  <div
    class="modal-backdrop"
    role="presentation"
    onclick={(event) => event.target === event.currentTarget && closeDictionaryModal()}
  >
    <div class="modal compact-modal" role="dialog" aria-modal="true" aria-labelledby="dictionary-modal-title">
      <header class="modal-header">
        <div>
          <span class="page-kicker">
            {editingDictionaryWordId === null ? "Новое слово" : "Изменение слова"}
          </span>
          <h2 id="dictionary-modal-title">
            {editingDictionaryWordId === null ? "Добавить в словарь" : "Редактировать слово"}
          </h2>
        </div>
        <button class="icon-button" onclick={closeDictionaryModal} title="Закрыть">
          <Icon name="close" size={19} />
        </button>
      </header>

      <form onsubmit={(event) => { event.preventDefault(); saveDictionaryWord(); }}>
        <div class="form-grid">
          <label class="form-field wide">
            <span>Правильное слово</span>
            <input
              bind:value={dictionaryWord}
              maxlength="64"
              placeholder="Например, согласование"
              autocomplete="off"
              spellcheck="false"
            />
            <small>Добавляйте правильную форму слова, а не вариант с ошибкой.</small>
          </label>

          <div class="form-field">
            <span>Категория</span>
            <SelectControl
              bind:value={dictionaryCategoryId}
              ariaLabel="Категория слова"
              options={[
                { value: "", label: "Без категории" },
                ...(data?.categories ?? []).map((category) => ({
                  value: String(category.id),
                  label: category.path,
                })),
              ]}
            />
          </div>

          <label class="form-field">
            <span>Приоритет</span>
            <input bind:value={dictionaryPriority} type="number" min="0" max="100" />
            <small>Чем выше число, тем важнее слово при выборе подсказки.</small>
          </label>
        </div>

        <div class="dictionary-options">
          <label>
            <input type="checkbox" bind:checked={dictionaryEnabled} />
            <span><strong>Слово включено</strong><small>Участвует в работе движка.</small></span>
          </label>
          <label>
            <input type="checkbox" bind:checked={dictionaryAutocomplete} />
            <span><strong>Автокомплит</strong><small>Предлагать при вводе начала слова.</small></span>
          </label>
          <label>
            <input type="checkbox" bind:checked={dictionaryAutocorrect} />
            <span><strong>Автокоррекция</strong><small>Исправлять похожие опечатки.</small></span>
          </label>
        </div>

        {#if dictionaryFormError}<p class="form-error">{dictionaryFormError}</p>{/if}

        <footer class="modal-footer">
          <button type="button" class="ghost-button" onclick={closeDictionaryModal}>Отмена</button>
          <button type="submit" class="primary-button" disabled={saving}>
            {saving
              ? "Сохраняем..."
              : editingDictionaryWordId === null
                ? "Добавить слово"
                : "Сохранить изменения"}
          </button>
        </footer>
      </form>
    </div>
  </div>
{/if}

{#if categoryModalOpen}
  <div
    class="modal-backdrop"
    role="presentation"
    onclick={(event) => event.target === event.currentTarget && closeCategoryModal()}
  >
    <div class="modal compact-modal" role="dialog" aria-modal="true" aria-labelledby="category-modal-title">
      <header class="modal-header">
        <div>
          <span class="page-kicker">
            {editingCategoryId === null ? "Новая папка" : "Изменение категории"}
          </span>
          <h2 id="category-modal-title">
            {editingCategoryId === null ? "Создать категорию" : "Переименовать категорию"}
          </h2>
        </div>
        <button class="icon-button" onclick={closeCategoryModal} title="Закрыть">
          <Icon name="close" size={19} />
        </button>
      </header>

      <form onsubmit={(event) => { event.preventDefault(); saveCategory(); }}>
        <div class="category-form-grid">
          <label class="form-field">
            <span>Название</span>
            <input
              bind:value={categoryName}
              maxlength="80"
              placeholder="Например, Сроки"
              autocomplete="off"
            />
            <small>Символ `/` используется для пути и недоступен в названии.</small>
          </label>

          {#if editingCategoryId === null}
            <div class="form-field">
              <span>Родительская категория</span>
              <SelectControl
                bind:value={categoryParentId}
                ariaLabel="Родительская категория"
                options={[
                  { value: "", label: "Корневая категория" },
                  ...(data?.categories ?? []).map((category) => ({
                    value: String(category.id),
                    label: category.path,
                  })),
                ]}
              />
              <small>Можно создать категорию на любом уровне дерева.</small>
            </div>
          {:else}
            <div class="category-path-preview">
              <span>Текущий путь</span>
              <strong>
                {data?.categories.find((category) => category.id === editingCategoryId)?.path ??
                  categoryName}
              </strong>
              <small>Пути дочерних категорий обновятся автоматически.</small>
            </div>
          {/if}
        </div>

        {#if formError}<p class="form-error">{formError}</p>{/if}

        <footer class="modal-footer">
          <button type="button" class="ghost-button" onclick={closeCategoryModal}>Отмена</button>
          <button type="submit" class="primary-button" disabled={saving}>
            {saving
              ? "Сохраняем..."
              : editingCategoryId === null
                ? "Создать категорию"
                : "Сохранить название"}
          </button>
        </footer>
      </form>
    </div>
  </div>
{/if}

<ConfirmDialog
  open={confirmation !== null}
  eyebrow={confirmation?.eyebrow}
  title={confirmation?.title ?? ""}
  message={confirmation?.message ?? ""}
  details={confirmation?.details ?? []}
  confirmLabel={confirmation?.confirmLabel}
  onConfirm={() => resolveConfirmation(true)}
  onCancel={() => resolveConfirmation(false)}
/>

{#if profileModalOpen}
  <div
    class="modal-backdrop"
    role="presentation"
    onclick={(event) => event.target === event.currentTarget && closeProfileModal()}
  >
    <div class="modal compact-modal" role="dialog" aria-modal="true" aria-labelledby="profile-modal-title">
      <header class="modal-header">
        <div>
          <span class="page-kicker">
            {editingProfileId === null ? "Новое пространство" : "Изменение профиля"}
          </span>
          <h2 id="profile-modal-title">
            {editingProfileId === null ? "Создать профиль" : "Переименовать профиль"}
          </h2>
        </div>
        <button class="icon-button" onclick={closeProfileModal} title="Закрыть">
          <Icon name="close" size={19} />
        </button>
      </header>

      <form onsubmit={(event) => { event.preventDefault(); saveProfile(); }}>
        <label class="form-field">
          <span>Название профиля</span>
          <input
            bind:value={profileName}
            maxlength="80"
            placeholder="Например, Типография"
            autocomplete="off"
          />
          {#if editingProfileId === null}
            <small>Новый профиль сразу станет активным.</small>
          {/if}
        </label>

        {#if profileFormError}<p class="form-error">{profileFormError}</p>{/if}

        <footer class="modal-footer">
          <button type="button" class="ghost-button" onclick={closeProfileModal}>Отмена</button>
          <button type="submit" class="primary-button" disabled={saving}>
            {saving
              ? "Сохраняем..."
              : editingProfileId === null
                ? "Создать профиль"
                : "Сохранить название"}
          </button>
        </footer>
      </form>
    </div>
  </div>
{/if}
