import { render } from "./libs/ElementBuilder.ts";
import { createEffect, createSignal } from "./libs/Signal.ts";
import { assertNotNull } from "./utils/assertion.ts";

const app = assertNotNull(document.querySelector("#app"), "expect #app");
render(<App />, app);

function App() {
  let counter = 0;
  const value = createSignal("something");

  let somethingRef: HTMLSpanElement | null = null;

  const fragment = (
    <>
      <div data-abcd="true" data-camelCase="false" data-with-dash="true">
        test2
      </div>
      <button
        type="button"
        on:click={() => {
          counter += 1;
          value.set(counter.toString());
        }}
      >
        inc
      </button>
      <span
        sub:innerText={value}
        with={(ref) => somethingRef = ref.element}
      />
      <br />
      <button
        type="button"
        on:click={() => {
          somethingRef?.remove();
          somethingRef = null;
        }}
      >
        remove something
      </button>
      <>
        <>
          <div>nested fragment</div>
        </>
      </>
    </>
  );

  return (
    <div id="app" class="none:definitely-not-a-magic-value">
      <div
        classes={["abcd", "arst"]}
        style={{ backgroundColor: "cornflowerblue" }}
      >
        test
      </div>
      {fragment}
      <Test value={1}>abcd{`123`}</Test>
    </div>
  );
}

function Test(props: { value: number; children: JSX.Element }) {
  return <div>{props.value.toString()} {props.children}</div>;
}
