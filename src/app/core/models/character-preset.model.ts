/** Набор символов для палитры ASCII — встроенный пресет либо
 * пользовательский (custom: true), добавленный через диалог. */
export interface CharacterPreset {
  label: string;
  value: string;
  description: string;
  custom?: boolean;
}

/** Встроенные пресеты — без пользовательских, те приходят из
 * `settings.service` (персистятся отдельно). */
export function builtInCharacterPresets(): CharacterPreset[] {
  return [
    {
      label: "Classic",
      value: " .:-=+*#%@",
      description: "Мягкие переходы",
    },
    {
      label: "Editorial",
      value:
        " .'`^ \",:;Il!i><~+_-?][}{1)(|\\/tfjrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@$",
      description: "Фотографическая четкость",
    },
    {
      label: "Raster",
      value: " `.,-':<>;+!*/?%&98#@",
      description: "Газетная текстура",
    },
    {
      label: "Blocks",
      value: " ░▒▓█",
      description: "Объемные фигуры",
    },
    {
      label: "Matrix",
      value: " 01",
      description: "Цифровой шифр",
    },
    {
      label: "Minimal",
      value: " .:-#",
      description: "Грубый силуэт",
    },
  ];
}