import { Injectable, signal } from "@angular/core";

export interface Notification {
  id: number;
  message: string;
  severity: "error" | "info";
}

const AUTO_DISMISS_MS = 6000;

/** Единственное место, куда стекаются ошибки из сервисов — раньше они
 * молча ловились в try/catch, и сбой выглядел как "ничего не произошло".
 * Теперь любая такая ошибка попадает сюда и показывается тостом. */
@Injectable({ providedIn: "root" })
export class NotificationsService {
  private readonly _items = signal<Notification[]>([]);
  readonly items = this._items.asReadonly();

  private nextId = 1;

  error(message: string): void {
    this.push(message, "error");
  }

  info(message: string): void {
    this.push(message, "info");
  }

  dismiss(id: number): void {
    this._items.update((list) => list.filter((item) => item.id !== id));
  }

  private push(message: string, severity: Notification["severity"]): void {
    const id = this.nextId++;
    this._items.update((list) => [...list, { id, message, severity }]);
    setTimeout(() => this.dismiss(id), AUTO_DISMISS_MS);
  }
}