import { execSync } from "child_process";
import process from "process";

const targetFile = process.argv[2];
const thumbprint = "DBFA862FAB75D3D7EC0F47AE5AA408F6DFBF1697";

if (!targetFile) {
  console.error("❌ Ошибка: не указан файл для подписи.");
  process.exit(1);
}

const timestampServers = [
  "http://timestamp.sectigo.com",
  "http://timestamp.globalsign.com/tsa/r45standard",
];

let signed = false;

for (const server of timestampServers) {
  console.log(`✍️ Попытка подписи файла через сервер: ${server}...`);
  try {
    execSync(
      `signtool.exe sign /fd sha256 /sha1 ${thumbprint} /tr "${server}" /td sha256 "${targetFile}"`,
      { stdio: "inherit", encoding: "utf-8" },
    );
    console.log(`✅ Файл успешно подписан со штампом времени от: ${server}`);
    signed = true;
    break;
  } catch (e) {
    console.warn(
      `⚠️ Сервер ${server} временно недоступен. Пробуем следующий...`,
    );
  }
}

// подписываем БЕЗ времени
if (!signed) {
  console.warn(
    "\n🚨 Все серверы времени недоступны. Подписываем БЕЗ штампа времени...",
  );
  try {
    execSync(
      `signtool.exe sign /fd sha256 /sha1 ${thumbprint} "${targetFile}"`,
      { stdio: "inherit", encoding: "utf-8" },
    );
    console.log("✅ Файл успешно подписан (без фиксации времени).");
  } catch (criticalError) {
    console.error("❌ Критическая ошибка подписи:", criticalError.message);
    process.exit(1);
  }
}
