# 🎨 Конвертер ASCII

> **Простой и удобный десктопный конвертер изображений в ASCII-арт**

[![Angular 21](https://img.shields.io/badge/Angular-21-blue?style=flat-square&logo=angular)](https://angular.dev)
[![Tauri 2](https://img.shields.io/badge/Tauri-2-2FFBD5?style=flat-square&logo=tauri)](https://tauri.app)
[![PrimeNG 21](https://img.shields.io/badge/PrimeNG-21-FF7216?style=flat-square&logo=prime)](https://primeng.org)
[![TypeScript](https://img.shields.io/badge/TypeScript-5.9-3178C6?style=flat-square&logo=typescript)](https://typescriptlang.org)
[![License](https://img.shields.io/badge/License-MIT-green?style=flat-square)](LICENSE)

---

## ✨ Возможности

- 🖼️ **Конвертация изображений** — превращайте любые картинки в ASCII-арт
- 🖥️ **Нативное десктопное приложение** — быстрое и лёгкое благодаря Tauri
- 🎯 **Удобный интерфейс** — современный UI на базе PrimeNG
- 🔄 **Автоматические обновления** — встроенный механизм обновления через Tauri Updater
- ⚡ **Высокая производительность** — Angular 21 + оптимизированный бэкенд на Rust

---

## 🛠️ Стек технологий

| Компонент | Технология |
|-----------|-----------|
| Фреймворк UI | [Angular 21](https://angular.dev) |
| Десктоп-обёртка | [Tauri 2](https://tauri.app) |
| UI-компоненты | [PrimeNG 21](https://primeng.org) |
| Шрифт | [Manrope](https://github.com/floriankarsten/manrope) |
| Язык | [TypeScript 5.9](https://typescriptlang.org) |
| Стили | SCSS |

---

## 📦 Установка

### Предварительные требования

- **Node.js** 18+ и **npm**
- **Rust** — [установка](https://www.rust-lang.org/tools/install)
- **Tauri CLI** — `npm install -g @tauri-apps/cli`
- **Windows**: Visual Studio Build Tools с C++ desktop development

### Быстрый старт

Добавить в PATH C:\Program Files (x86)\Microsoft Visual Studio\Installer

```bash
# 1. Клонируйте репозиторий
git clone https://github.com/CRUSTALiYm/ASCII.git
cd ASCII

# 2. Установите зависимости
npm install

# 3. Запустите режим разработки
npm run tauri:start
```

---

## 🚀 Скрипты

| Команда | Описание |
|---------|----------|
| `npm run tauri:start` | Запуск приложения в режиме разработки |
| `npm run start` | Запуск Angular dev-сервера (для веба) |
| `npm run build` | Сборка Angular-приложения |
| `npm run tauri:build:all` | Сборка для всех архитектур (x64, x86, ARM64) |
| `npm run tauri:build:x64` | Сборка для x64 Windows |
| `npm run tauri:build:x86` | Сборка для x86 Windows |
| `npm run tauri:build:arm64` | Сборка для ARM64 Windows |

---

## ⚙️ Конфигурация

Скопируйте `.env.example` в `.env` и настройте переменные для автообновлений:

```env
TAURI_SIGNING_PRIVATE_KEY=""
TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
```

> 💡 Сгенерировать ключи: `npx tauri signer generate`

---

## 📁 Структура проекта

```
ASCII/
├── src/                 # Исходный код Angular
│   ├── app/             # Компоненты, сервисы, модули
│   ├── assets/          # Статические ресурсы
│   └── styles.scss      # Глобальные стили
├── src-tauri/           # Код Tauri (Rust)
│   ├── src/             # Rust-источник
│   └── Cargo.toml       # Зависимости Rust
├── scripts/             # Скрипты сборки
├── package.json         # Зависимости и скрипты
└── angular.json         # Конфигурация Angular
```

---

## 🎯 Рекомендуемая IDE

| Расширение | Ссылка |
|------------|--------|
| Tauri | [tauri-vscode](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) |
| rust-analyzer | [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer) |
| Angular Language Service | [ng-template](https://marketplace.visualstudio.com/items?itemName=Angular.ng-template) |

**VS Code** — рекомендуемая IDE для разработки.

---

## 📄 Лицензия

Проект распространяется под лицензией [MIT](LICENSE-MIT).
