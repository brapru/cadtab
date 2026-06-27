// Rasterize a standalone SVG string to a PNG blob via the browser canvas —
// identical on desktop (WKWebView) and web (Chromium/Firefox). This is platform
// glue (Image + canvas): it needs a real browser, so it is exercised in the app,
// not in jsdom unit tests.

/// Draw `svg` onto a canvas and return a PNG blob. `scale` multiplies the SVG's
/// intrinsic pixel size for higher-resolution output.
export async function svgToPngBlob(svg: string, scale = 2): Promise<Blob> {
  const url = URL.createObjectURL(
    new Blob([svg], { type: "image/svg+xml;charset=utf-8" }),
  );
  try {
    const img = new Image();
    await new Promise<void>((resolve, reject) => {
      img.onload = () => resolve();
      img.onerror = () => reject(new Error("failed to rasterize SVG"));
      img.src = url;
    });

    const canvas = document.createElement("canvas");
    canvas.width = Math.max(1, Math.round(img.width * scale));
    canvas.height = Math.max(1, Math.round(img.height * scale));
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("no 2d canvas context");
    // Paint the sheet white first so the PNG is not transparent.
    ctx.fillStyle = "#ffffff";
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.drawImage(img, 0, 0, canvas.width, canvas.height);

    return await new Promise<Blob>((resolve, reject) =>
      canvas.toBlob(
        (b) =>
          b ? resolve(b) : reject(new Error("canvas.toBlob returned null")),
        "image/png",
      ),
    );
  } finally {
    URL.revokeObjectURL(url);
  }
}
