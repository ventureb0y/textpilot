export type ProfileSummary = {
  id: number;
  name: string;
  isActive: boolean;
  categoryCount: number;
  phraseCount: number;
  dictionaryCount: number;
};

export type CategorySummary = {
  id: number;
  parentId: number | null;
  name: string;
  path: string;
  phraseCount: number;
  wordCount: number;
};

export type PhraseSummary = {
  id: number;
  categoryId: number | null;
  title: string;
  snippet: string;
  body: string;
  description: string;
  isEnabled: boolean;
};

export type DictionaryWordSummary = {
  id: number;
  categoryId: number | null;
  word: string;
  priority: number;
  isEnabled: boolean;
  autocompleteEnabled: boolean;
  autocorrectEnabled: boolean;
};

export type DashboardData = {
  profiles: ProfileSummary[];
  activeProfileId: number;
  categories: CategorySummary[];
  phrases: PhraseSummary[];
  dictionaryWords: DictionaryWordSummary[];
  dictionaryCount: number;
  isPaused: boolean;
  engineAvailable: boolean;
  dictionaryAutocompleteEnabled: boolean;
  quickSearchEnabled: boolean;
};

export type ExportResult = {
  path: string;
  profileCount: number;
  categoryCount: number;
  phraseCount: number;
  wordCount: number;
};

export type ImportPreview = {
  version: number;
  targetProfileName: string | null;
  profileCount: number;
  categoryCount: number;
  phraseCount: number;
  wordCount: number;
  snippetConflicts: number;
  wordConflicts: number;
};

export type ImportResult = {
  profileCount: number;
  categoryCount: number;
  phraseCount: number;
  wordCount: number;
  skippedConflicts: number;
  backupPath: string | null;
};

export type BackupSummary = {
  fileName: string;
  sizeBytes: number;
  modifiedAtMs: number;
};

export type RestoreBackupResult = {
  safetyBackupPath: string;
};

export type ProfileSnapshotSummary = {
  fileName: string;
  profileId: number;
  profileName: string;
  sizeBytes: number;
  modifiedAtMs: number;
};

export type RestoreProfileSnapshotResult = {
  safetySnapshotPath: string;
};

export type NavigationSection = "phrases" | "dictionary" | "profiles" | "settings";
