# Обновления TextPilot

TextPilot использует официальный Tauri Updater. Метаданные публикуются через
GitVerse Pages, а NSIS-установщик хранится в GitVerse Releases.

## Первый подписанный выпуск

Версия 0.0.5 — первая версия со встроенным updater. Пользователям версии 0.0.4
нужно установить ее вручную. Автоматические обновления начнут работать со
следующего выпуска.

## Проверка версий

Перед созданием тега все четыре версии должны совпадать:

- `package.json`;
- `src-tauri/Cargo.toml`;
- запись TextPilot в `src-tauri/Cargo.lock`;
- `src-tauri/tauri.conf.json`.

Проверка:

```powershell
./scripts/check-release-version.ps1 -Tag v0.0.5
```

## Подписанная сборка

Приватный ключ хранится вне репозитория:

```text
%USERPROFILE%\.tauri\textpilot.key
```

Сборка:

```powershell
./scripts/build-signed-update.ps1
```

Скрипт запросит пароль ключа без отображения ввода, выполнит проверки, соберет
NSIS и убедится, что рядом с установщиком создан файл `.sig`.

## Публикация первого релиза

1. Создать и отправить тег `v0.0.5`.
2. Создать публичный GitVerse Release для этого тега.
3. Загрузить установщик `.exe` и соответствующий файл `.exe.sig`.
4. Скопировать `browser_download_url` установщика.
5. Сгенерировать манифест:

```powershell
./scripts/generate-update-manifest.ps1 `
  -Version 0.0.5 `
  -InstallerUrl "https://api.gitverse.ru/repos/ventureb0y/textpilot/releases/RELEASE_ID/assets/ASSET_ID/download" `
  -SignaturePath "src-tauri/target/release/bundle/nsis/TextPilot_0.0.5_x64-setup.exe.sig" `
  -Notes "Первый выпуск TextPilot со встроенными обновлениями."
```

6. Проверить `site/latest.json`, закоммитить и отправить его в `main`.
7. В настройках GitVerse Pages выбрать ветку `main` и каталог `/site`.

Постоянный endpoint updater:

```text
https://ventureb0y.gitverse.site/textpilot/latest.json
```

Манифест публикуется только после того, как установщик доступен по публичной
ссылке. Приватный ключ и его пароль никогда не добавляются в Git.
