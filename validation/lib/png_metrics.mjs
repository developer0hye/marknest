import pixelmatch from "pixelmatch";
import { PNG } from "pngjs";

const BLANK_PIXEL_THRESHOLD = 250;
const BLANK_PAGE_RATIO = 0.995;
const EDGE_SAMPLE_SIZE = 2;

function isInkPixel(red, green, blue, alpha) {
  if (alpha === 0) {
    return false;
  }
  return !(
    red >= BLANK_PIXEL_THRESHOLD &&
    green >= BLANK_PIXEL_THRESHOLD &&
    blue >= BLANK_PIXEL_THRESHOLD
  );
}

function ratioWithSafeDenominator(numerator, denominator) {
  if (denominator <= 0) {
    return 0;
  }
  return numerator / denominator;
}

function paddedPng(sourcePng, width, height) {
  const padded = new PNG({ width, height, colorType: 6 });
  padded.data.fill(255);
  PNG.bitblt(sourcePng, padded, 0, 0, sourcePng.width, sourcePng.height, 0, 0);
  return padded;
}

export function isNearBlankPage(pagePng) {
  let whitePixels = 0;
  const totalPixels = pagePng.width * pagePng.height;

  for (let offset = 0; offset < pagePng.data.length; offset += 4) {
    if (
      !isInkPixel(
        pagePng.data[offset],
        pagePng.data[offset + 1],
        pagePng.data[offset + 2],
        pagePng.data[offset + 3],
      )
    ) {
      whitePixels += 1;
    }
  }

  return ratioWithSafeDenominator(whitePixels, totalPixels) >= BLANK_PAGE_RATIO;
}

export function buildPageMetrics(pagePng) {
  let rightEdgeInkPixels = 0;
  let bottomEdgeInkPixels = 0;

  for (let y = 0; y < pagePng.height; y += 1) {
    for (let x = Math.max(0, pagePng.width - EDGE_SAMPLE_SIZE); x < pagePng.width; x += 1) {
      const offset = (y * pagePng.width + x) * 4;
      if (
        isInkPixel(
          pagePng.data[offset],
          pagePng.data[offset + 1],
          pagePng.data[offset + 2],
          pagePng.data[offset + 3],
        )
      ) {
        rightEdgeInkPixels += 1;
      }
    }
  }

  for (let y = Math.max(0, pagePng.height - EDGE_SAMPLE_SIZE); y < pagePng.height; y += 1) {
    for (let x = 0; x < pagePng.width; x += 1) {
      const offset = (y * pagePng.width + x) * 4;
      if (
        isInkPixel(
          pagePng.data[offset],
          pagePng.data[offset + 1],
          pagePng.data[offset + 2],
          pagePng.data[offset + 3],
        )
      ) {
        bottomEdgeInkPixels += 1;
      }
    }
  }

  return {
    nearBlank: isNearBlankPage(pagePng),
    rightEdgeInkRatio: ratioWithSafeDenominator(
      rightEdgeInkPixels,
      pagePng.height * Math.min(EDGE_SAMPLE_SIZE, pagePng.width),
    ),
    bottomEdgeInkRatio: ratioWithSafeDenominator(
      bottomEdgeInkPixels,
      pagePng.width * Math.min(EDGE_SAMPLE_SIZE, pagePng.height),
    ),
  };
}

export function comparePngBuffers(leftBuffer, rightBuffer) {
  const leftPng = PNG.sync.read(leftBuffer);
  const rightPng = PNG.sync.read(rightBuffer);
  const width = Math.max(leftPng.width, rightPng.width);
  const height = Math.max(leftPng.height, rightPng.height);
  const paddedLeft = paddedPng(leftPng, width, height);
  const paddedRight = paddedPng(rightPng, width, height);
  const diffPng = new PNG({ width, height, colorType: 6 });

  const diffPixels = pixelmatch(
    paddedLeft.data,
    paddedRight.data,
    diffPng.data,
    width,
    height,
    { threshold: 0.1 },
  );

  return {
    diffPixels,
    diffRatio: ratioWithSafeDenominator(diffPixels, width * height),
    diffPng: PNG.sync.write(diffPng),
  };
}
