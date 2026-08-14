// Real-time divergence point auto-detection from free-text prompts.
export function detectDivergenceFromPrompt(prompt) {
  if (!prompt || typeof prompt !== "string") return null;
  const lower = prompt.toLowerCase();

  // 1. Explicit BCE/BC: e.g. "3200 BCE", "44 BC"
  const bceMatch = lower.match(/\b(\d{1,7})\s*(bce|bc)\b/i);
  if (bceMatch) {
    const y = parseInt(bceMatch[1], 10);
    if (!isNaN(y)) return -y + 1; // astronomical BCE year
  }

  // 2. Explicit CE/AD: e.g. "1492 CE", "476 AD"
  const ceMatch = lower.match(/\b(\d{1,4})\s*(ce|ad)\b/i);
  if (ceMatch) {
    const y = parseInt(ceMatch[1], 10);
    if (!isNaN(y)) return y;
  }

  // 3. Explicit 4-digit year (1000-2100): e.g. "in 1943", "1865", "1776", "1914"
  const yearMatch = lower.match(/\b(1\d{3}|20\d{2})\b/);
  if (yearMatch) {
    const y = parseInt(yearMatch[1], 10);
    if (!isNaN(y) && y >= 1000 && y <= 2100) return y;
  }

  // 4. Keyword / Historical event matching
  if (/nazi|hitler|wwii|world war 2|world war ii|axis|holocaust|stalingrad/i.test(lower)) {
    return 1939;
  }
  if (/wwi|world war 1|world war i|franz ferdinand|sarajevo/i.test(lower)) {
    return 1914;
  }
  if (/columbus|americas.*never reached|never reached.*americas/i.test(lower)) {
    return 1492;
  }
  if (/rome never fell|fall of rome|fall of the roman empire|western roman/i.test(lower)) {
    return 476;
  }
  if (/caesar|julius caesar|rubicon/i.test(lower)) {
    return -43; // 44 BCE
  }
  if (/alexander the great|alexander.*died|alexander.*lived/i.test(lower)) {
    return -322; // 323 BCE
  }
  if (/constantinople|byzantine|fall of constantinople/i.test(lower)) {
    return 1453;
  }
  if (/american revolution|declaration of independence/i.test(lower)) {
    return 1776;
  }
  if (/civil war|confederacy|gettysburg|lincoln/i.test(lower)) {
    return 1861;
  }
  if (/french revolution|bastille|guillotine/i.test(lower)) {
    return 1789;
  }
  if (/napoleon|waterloo|bonaparte/i.test(lower)) {
    return 1815;
  }
  if (/cuban missile|cold war.*hot|hot.*cold war/i.test(lower)) {
    return 1962;
  }
  if (/ussr|soviet union|berlin wall|collapse of the soviet/i.test(lower)) {
    return 1989;
  }
  if (/mongol|genghis khan|baghdad/i.test(lower)) {
    return 1258;
  }
  if (/black death|bubonic plague/i.test(lower)) {
    return 1347;
  }
  if (/islam|muhammad|hijra|caliphate/i.test(lower)) {
    return 622;
  }
  if (/printing press|gutenberg/i.test(lower)) {
    return 1440;
  }
  if (/moon landing|apollo 11/i.test(lower)) {
    return 1969;
  }
  if (/covid|pandemic/i.test(lower)) {
    return 2020;
  }
  if (/ukraine|russia.*invade|invasion of ukraine/i.test(lower)) {
    return 2022;
  }
  if (/fire|homo erectus/i.test(lower)) {
    return -1900000;
  }
  if (/writing|sumer|mesopotamia/i.test(lower)) {
    return -3199;
  }

  return null;
}
