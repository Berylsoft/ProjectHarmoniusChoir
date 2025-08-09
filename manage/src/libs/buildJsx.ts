import { unreachable } from "../utils/assertion.ts";
import { AnyNode, E } from "./ElementBuilder.ts";

export const jsxFragment = { ___jsxFragment: 1 };
// deno-lint-ignore no-explicit-any
type Props = any;

export function buildJsx<
  K extends keyof HTMLElementTagNameMap,
>(
  element:
    | K
    | typeof jsxFragment
    | ((props: Props) => AnyNode),
  props: Props,
  ...children: AnyNode[]
): AnyNode {
  if (typeof element === "string") {
    const e = E(element as K).children(...children);

    for (const key in props) {
      switch (key) {
        case "id": {
          e.id(props[key]);
          break;
        }
        case "class": {
          e.classes(props[key]);
          break;
        }
        case "classes": {
          e.classes(...props[key]);
          break;
        }
        case "style": {
          e.styleMany(props[key]);
          break;
        }
        case "with": {
          e.with(props[key]);
          break;
        }
        default: {
          if (key.startsWith("data-")) {
            let dataKey = key.replace(/^data-/, "");
            if (dataKey.includes("-")) {
              let dataKeyCamelCase = "";
              let i = 0;
              while (i < dataKey.length) {
                if (dataKey[i] == "-") {
                  if (i < dataKey.length - 1) {
                    dataKeyCamelCase += dataKey[i + 1].toUpperCase();
                    i += 2;
                  } else {
                    i++;
                  }
                } else {
                  dataKeyCamelCase += dataKey[i];
                  i++;
                }
              }
              dataKey = dataKeyCamelCase;
            }
            e.data(dataKey, props[key]);
          } else if (key.startsWith("on:")) {
            e.on(
              key.replace(/^on:/, "") as keyof HTMLElementEventMap,
              props[key],
            );
          } else if (key.startsWith("sub:jsxContent")) {
            const signal = props[key] as Signal<JSX.Element>;

            e.children(signal.get());

            const subscriber = (v: JSX.Element) => {
              e.children(v);
            };
            signal.subscribe(new WeakRef(subscriber));
            (e.element as Record<string, unknown>)[key] = subscriber;
          } else if (key.startsWith("sub:")) {
            const to = key.replace(
              /^sub:/,
              "",
            ) as keyof HTMLElementTagNameMap[K];

            type Value = HTMLElementTagNameMap[K][typeof to];
            const signal = props[key] as Signal<Value>;

            e.element[to] = signal.get();

            const theElement = e.element;
            const subscriber = (v: Value) => {
              theElement[to] = v;
            };
            signal.subscribe(new WeakRef(subscriber));
            (theElement as Record<string, unknown>)[key] = subscriber;
          } else {
            (e.element as Record<string, unknown>)[key] = props[key];
          }
          break;
        }
      }
    }

    return e;
  } else if (element === jsxFragment) {
    return children;
  } else if (typeof element === "function") {
    if (children) {
      if (props == null) {
        props = {};
      }
      props.children = children;
    }
    return element(props);
  } else {
    unreachable();
  }
}
