import { render } from "./libs/ElementBuilder.ts";
import { Auth } from "./auth.tsx";
import { readNav, writeNav } from "./nav.ts";
import { setRedirectAuth, setRedirectAuthSudo } from "./redirect.ts";
import { assertNotNull, unreachable } from "./utils/assertion.ts";
import { Level, setLevel } from "./utils/logging.ts";
import { Manage } from "./manage.tsx";

setLevel(Level.Trace);

const app = assertNotNull(document.querySelector("#app"), "expect #app");
render(<App />, app);

function App() {
  const nav = readNav();
  if (nav.searchParams.get("default")) {
    nav.searchParams.delete("default");
    writeNav(nav, false);
  }
  setRedirectAuth(() => {
    nav.host = "auth";
    writeNav(nav, true);
    unreachable("reload");
  });
  setRedirectAuthSudo(() => {
    nav.host = "auth";
    nav.search = "?sudo=true";
    writeNav(nav, true);
    unreachable("reload");
  });

  switch (nav.host) {
    case "auth":
      return (
        <div id="app">
          <Auth nav={nav} />
        </div>
      );
    case "manage":
      return (
        <div id="app">
          <Manage nav={nav} />
        </div>
      );

    default:
      return (
        <div id="app">
          unreachable
        </div>
      );
  }
}
