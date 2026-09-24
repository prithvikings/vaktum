export function cleanupTranscript(input: string): string {
  let text = input.replace(/\s+/g, " ").trim();

  if (!text) return "";

  text = text
    .replace(/\s+([,.!?;:])/g, "$1")
    .replace(/([,.!?;:])([^\s])/g, "$1 $2");

  const tokens = text.split(" ");
  const deduped: string[] = [];

  for (const token of tokens) {
    const previous = deduped[deduped.length - 1];
    const currentWord = normalizeWord(token);
    const previousWord = previous ? normalizeWord(previous) : "";

    if (currentWord && previousWord && currentWord === previousWord) {
      continue;
    }

    deduped.push(token);
  }

  text = deduped.join(" ");

  return text.replace(/(^|[.!?]\s+)(\S)/g, (_, prefix, first) => {
    return prefix + first.toUpperCase();
  });
}

function normalizeWord(token: string): string {
  return token.replace(/^[.,!?;:]+|[.,!?;:]+$/g, "").toLowerCase();
}
