import { AsciiParamsInput } from "./ascii-params.model";

/** Один сохранённый результат. `settings` — полный снимок параметров на
 * момент сохранения; restore ставит его как есть, без частичного мерджа. */
export interface HistoryItem {
  id: string;
  name: string;
  path: string;
  createdAt: string;
  settings: AsciiParamsInput;
  /** Цвет текста превью (appearance.service) — на бэкенд не уходит, но
   * нужен, чтобы restore точно повторил внешний вид результата. */
  tone: string;
}