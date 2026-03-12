export function enableDocumentScroll(documentRef) {
  if (!documentRef?.body || !documentRef?.documentElement) {
    return () => {};
  }

  const body = documentRef.body;
  const html = documentRef.documentElement;
  const root = documentRef.getElementById?.("root");

  const previous = {
    bodyOverflow: body.style.overflow,
    bodyHeight: body.style.height,
    htmlOverflow: html.style.overflow,
    htmlHeight: html.style.height,
    rootHeight: root?.style?.height ?? "",
  };

  body.style.overflow = "auto";
  body.style.height = "auto";
  html.style.overflow = "auto";
  html.style.height = "auto";
  if (root?.style) {
    root.style.height = "auto";
  }

  return () => {
    body.style.overflow = previous.bodyOverflow;
    body.style.height = previous.bodyHeight;
    html.style.overflow = previous.htmlOverflow;
    html.style.height = previous.htmlHeight;
    if (root?.style) {
      root.style.height = previous.rootHeight;
    }
  };
}
