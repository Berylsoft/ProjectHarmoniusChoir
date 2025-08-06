import { render } from "./libs/ElementBuilder.ts";
import { assertNotNull } from "./utils/assertion.ts";

const app = assertNotNull(document.querySelector("#app"), "expect #app");
render(<App />, app);

function App() {
  return (
    <div id="app">
      App
    </div>
  );
}
