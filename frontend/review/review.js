/* 审查页共享脚本：主题 / 根字号切换与 token 值回填。 */
const root = document.documentElement;
const cssVar = (name) => getComputedStyle(root).getPropertyValue(name).trim();

/**
 * 当前根字号（px）。优先读本页写入的 inline 值：同一任务内刚改完 font-size 时
 * getComputedStyle 仍返回旧值，会导致换算出的 px 慢一拍。
 */
function rootFontPx() {
  const inline = parseFloat(root.style.fontSize);
  if (!Number.isNaN(inline)) return inline;
  return parseFloat(getComputedStyle(root).fontSize) || 16;
}

/** rem → px（按当前根字号），非 rem 原样返回。 */
function toPx(value) {
  if (value.endsWith("rem")) return (parseFloat(value) * rootFontPx()).toFixed(1) + "px";
  return value;
}

function refreshValues() {
  document.querySelectorAll("[data-color-value]").forEach((el) => {
    el.textContent = cssVar(el.dataset.colorValue) || "—";
  });
  document.querySelectorAll("[data-rem-value]").forEach((el) => {
    el.textContent = toPx(cssVar(el.dataset.remValue));
  });
  document.querySelectorAll("[data-raw-value]").forEach((el) => {
    el.textContent = cssVar(el.dataset.rawValue);
  });
}

function setTheme(theme) {
  root.dataset.theme = theme;
  document.querySelectorAll("[data-theme-btn]").forEach((b) =>
    b.setAttribute("aria-pressed", String(b.dataset.themeBtn === theme)),
  );
  refreshValues();
}

function setFontSize(px) {
  root.style.fontSize = px + "px";
  document.querySelectorAll("[data-size-btn]").forEach((b) =>
    b.setAttribute("aria-pressed", String(b.dataset.sizeBtn === px)),
  );
  refreshValues();
}

document.querySelectorAll("[data-theme-btn]").forEach((b) =>
  b.addEventListener("click", () => setTheme(b.dataset.themeBtn)),
);
document.querySelectorAll("[data-size-btn]").forEach((b) =>
  b.addEventListener("click", () => setFontSize(b.dataset.sizeBtn)),
);

setTheme("dark");
setFontSize(17);
