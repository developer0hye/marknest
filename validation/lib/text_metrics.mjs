function stripHtmlComments(markdownText) {
  return markdownText.replace(/<!--[\s\S]*?-->/g, " ");
}

function decodeHtmlEntities(markdownText) {
  return String(markdownText ?? "").replace(
    /&(#x[0-9a-f]+|#\d+|middot|amp|lt|gt|quot|apos|nbsp|ensp|emsp|thinsp);/gi,
    (match, entityBody) => {
      const normalizedEntity = String(entityBody).toLowerCase();
      if (normalizedEntity === "middot") {
        return "·";
      }
      if (normalizedEntity === "amp") {
        return "&";
      }
      if (normalizedEntity === "lt") {
        return "<";
      }
      if (normalizedEntity === "gt") {
        return ">";
      }
      if (normalizedEntity === "quot") {
        return "\"";
      }
      if (normalizedEntity === "apos") {
        return "'";
      }
      if (normalizedEntity === "nbsp") {
        return " ";
      }
      if (
        normalizedEntity === "ensp"
        || normalizedEntity === "emsp"
        || normalizedEntity === "thinsp"
      ) {
        return " ";
      }
      if (normalizedEntity.startsWith("#x")) {
        const codePoint = Number.parseInt(normalizedEntity.slice(2), 16);
        return Number.isNaN(codePoint) ? match : String.fromCodePoint(codePoint);
      }
      if (normalizedEntity.startsWith("#")) {
        const codePoint = Number.parseInt(normalizedEntity.slice(1), 10);
        return Number.isNaN(codePoint) ? match : String.fromCodePoint(codePoint);
      }
      return match;
    },
  );
}

function stripMarkdownImageSyntax(markdownText) {
  return markdownText
    .replace(/!\[[^\]]*]\(([^)]+)\)/g, " ")
    .replace(/!\[[^\]]*]\[[^\]]*]/g, " ")
    .replace(/!\[[^\]]*]/g, " ");
}

function replaceMarkdownLinksWithText(markdownText) {
  return markdownText
    .replace(/\[([^\]]+)]\(([^)]+)\)/g, (_match, label) => (
      String(label).trim().startsWith("!") ? " " : label
    ))
    .replace(/\[([^\]]+)]\[[^\]]*]/g, (_match, label) => (
      String(label).trim().startsWith("!") ? " " : label
    ));
}

function stripLinkDefinitionLines(markdownText) {
  return markdownText
    .split("\n")
    .filter((line) => !/^\s*\[[^\]]+]:\s*\S+/.test(line))
    .join("\n");
}

function stripInlineCodeFenceMarkers(markdownText) {
  return markdownText.replace(/`{1,3}/g, " ");
}

function stripMarkdownEmphasis(markdownText) {
  return String(markdownText ?? "")
    .replace(/\*\*([^*]+)\*\*/g, "$1")
    .replace(/__([^_]+)__/g, "$1")
    .replace(/\*([^*]+)\*/g, "$1")
    .replace(/_([^_]+)_/g, "$1")
    .replace(/~~([^~]+)~~/g, "$1");
}

function stripRawHtmlTags(markdownText) {
  return markdownText
    .replace(/<\/?(sup|sub)>/gi, "")
    .replace(/<[^>]+>/g, " ");
}

function stripUrls(markdownText) {
  return markdownText.replace(/\bhttps?:\/\/\S+/gi, " ");
}

function normalizeSearchText(text) {
  return String(text ?? "")
    .toLowerCase()
    .replace(/[\u2000-\u206F]/g, " ")
    .replace(/[^\p{L}\p{N}\s._+-]+/gu, " ")
    .replace(/\s[-+_]+\s/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

function tokenizeText(text) {
  const matches = normalizeSearchText(text).match(/[\p{L}\p{N}][\p{L}\p{N}_+-]{2,}/gu) ?? [];
  const orderedTokens = [];
  const seenTokens = new Set();
  for (const token of matches) {
    const letterCount = [...token].filter((character) => /\p{L}/u.test(character)).length;
    if (letterCount < 2 || /\d/.test(token)) {
      continue;
    }
    if (seenTokens.has(token)) {
      continue;
    }
    seenTokens.add(token);
    orderedTokens.push(token);
  }
  return orderedTokens;
}

function cleanInlineMarkdown(text) {
  return stripMarkdownEmphasis(
    replaceMarkdownLinksWithText(stripMarkdownImageSyntax(String(text ?? ""))),
  );
}

function uniqueInOrder(values) {
  const orderedValues = [];
  const seenValues = new Set();
  for (const value of values) {
    if (seenValues.has(value)) {
      continue;
    }
    seenValues.add(value);
    orderedValues.push(value);
  }
  return orderedValues;
}

function splitMarkdownIntoVisibleLines(markdownText) {
  const lines = decodeHtmlEntities(stripHtmlComments(String(markdownText ?? "")))
    .replace(/\r\n/g, "\n")
    .split("\n");
  const visibleLines = [];
  const headingValues = [];
  const localImageReferences = [];
  const remoteImageReferences = [];
  let inFence = false;

  for (const line of lines) {
    const trimmedLine = line.trim();
    if (/^(```|~~~)/.test(trimmedLine)) {
      inFence = !inFence;
      continue;
    }

    if (!inFence) {
      const headingMatch = /^(#{1,3})\s+(.+?)\s*$/.exec(trimmedLine);
      if (headingMatch) {
        headingValues.push(
          normalizeSearchText(
            stripUrls(stripRawHtmlTags(cleanInlineMarkdown(headingMatch[2]))),
          ),
        );
      }

      for (const markdownImageMatch of line.matchAll(/!\[[^\]]*]\(([^)\s]+)(?:\s+"[^"]*")?\)/g)) {
        const reference = markdownImageMatch[1].trim();
        if (/^https?:\/\//i.test(reference)) {
          remoteImageReferences.push(reference);
        } else {
          localImageReferences.push(reference);
        }
      }

      for (const htmlImageMatch of line.matchAll(/<img[^>]+src=(["']?)([^"'\s>]+)\1[^>]*>/gi)) {
        const reference = htmlImageMatch[2].trim();
        if (/^https?:\/\//i.test(reference)) {
          remoteImageReferences.push(reference);
        } else {
          localImageReferences.push(reference);
        }
      }

      if (/^\s*\[[^\]]+]:\s*\S+/.test(line)) {
        continue;
      }
      if (/^\s*\|.*\|\s*$/.test(line) || /^\s*[:\-| ]+\s*$/.test(line)) {
        continue;
      }
      visibleLines.push(line);
    }
  }

  return {
    headings: uniqueInOrder(headingValues.filter((value) => value.length > 0)),
    localImageReferences: uniqueInOrder(localImageReferences),
    remoteImageReferences: uniqueInOrder(remoteImageReferences),
    visibleText: visibleLines.join("\n"),
  };
}

export function buildSourceSnapshot(markdownText) {
  const visible = splitMarkdownIntoVisibleLines(markdownText);
  const normalizedVisibleText = stripUrls(
    stripRawHtmlTags(
      stripInlineCodeFenceMarkers(
        stripLinkDefinitionLines(
          stripMarkdownEmphasis(
            replaceMarkdownLinksWithText(stripMarkdownImageSyntax(visible.visibleText)),
          ),
        ),
      ),
    ),
  );

  return {
    headings: visible.headings,
    localImageReferences: visible.localImageReferences,
    remoteImageReferences: visible.remoteImageReferences,
    sourceTokens: tokenizeText(normalizedVisibleText),
  };
}

export function calculateTokenCoverage(sourceSnapshot, extractedPdfText) {
  const sourceTokens = Array.isArray(sourceSnapshot?.sourceTokens)
    ? sourceSnapshot.sourceTokens
    : [];
  if (sourceTokens.length === 0) {
    return { coverage: 1, missingTokens: [] };
  }

  const outputTokenSet = new Set(tokenizeText(extractedPdfText));
  const missingTokens = sourceTokens.filter((token) => !outputTokenSet.has(token));
  return {
    coverage: (sourceTokens.length - missingTokens.length) / sourceTokens.length,
    missingTokens,
  };
}

export function calculateHeadingCoverage(sourceSnapshot, extractedPdfText) {
  const sourceHeadings = Array.isArray(sourceSnapshot?.headings) ? sourceSnapshot.headings : [];
  if (sourceHeadings.length === 0) {
    return { coverage: 1, missingHeadings: [] };
  }

  const normalizedOutputText = normalizeSearchText(extractedPdfText);
  const missingHeadings = sourceHeadings.filter((heading) => !normalizedOutputText.includes(heading));
  return {
    coverage: (sourceHeadings.length - missingHeadings.length) / sourceHeadings.length,
    missingHeadings,
  };
}

export function splitExtractedPdfPages(extractedPdfText) {
  return String(extractedPdfText ?? "")
    .split("\f")
    .map((pageText) => pageText.trim())
    .filter((pageText, index, pages) => pageText.length > 0 || index < pages.length - 1);
}

export function hasMeaningfulPageText(pageText) {
  return tokenizeText(pageText).length > 0;
}
