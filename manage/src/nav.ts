import { debug, trace } from "./utils/logging.ts";

export function readNav(): URL {
  const defNav = "app://manage/?default=true";
  const params = new URLSearchParams(globalThis.location.search);
  const navStr = params.get("nav") ?? defNav;
  try {
    const nav = new URL(navStr);
    return nav;
  } catch (e) {
    console.warn("unexpected invalid nav: %o", e);
    return new URL(defNav);
  }
}

export function writeNav(nav: URL, reload?: boolean) {
  debug("update nav to: %o", nav);
  const params = new URLSearchParams(globalThis.location.search);
  params.set("nav", nav.toString());

  const newSearch = `?${params.toString()}`;

  globalThis.history.pushState(
    null,
    "",
    globalThis.location.pathname + newSearch,
  );

  if (reload) {
    trace("reload");
    globalThis.location.search = newSearch;
  }
}
