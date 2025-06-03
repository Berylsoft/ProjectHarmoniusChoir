export type AnyNode =
  | Node
  | string
  | ElementBuilder<HTMLElement>
  | AnyNode[];

export class ElementBuilder<T extends HTMLElement> {
  element: T;

  constructor(element: T) {
    this.element = element;
  }

  static fromTagName<
    K extends keyof HTMLElementTagNameMap,
    T extends HTMLElementTagNameMap[K],
  >(tagName: K, options?: ElementCreationOptions): ElementBuilder<T> {
    return new ElementBuilder(document.createElement(tagName, options) as T);
  }

  children(
    ...nodes: AnyNode[]
  ): ElementBuilder<T> {
    this.element.replaceChildren();

    const stack = [nodes];
    while (stack.length > 0) {
      const top = stack.at(-1)!;
      if (top.length === 0) {
        stack.pop();
        continue;
      }

      const node = top.shift()!;
      if (node instanceof ElementBuilder) {
        this.element.append(node.element);
      } else if (node instanceof Array) {
        stack.push(node);
      } else {
        this.element.append(node);
      }
    }
    return this;
  }

  attr(name: string, value?: string): ElementBuilder<T> {
    if (value === undefined) {
      this.element.removeAttribute(name);
    } else {
      this.element.setAttribute(name, value);
    }
    return this;
  }

  data(name: string, value: string, remove?: boolean): ElementBuilder<T> {
    if (remove) {
      delete this.element.dataset[name];
    } else {
      this.element.dataset[name] = value;
    }
    return this;
  }

  id(id: string): ElementBuilder<T> {
    this.element.id = id;
    return this;
  }

  classes(...classes: string[]): ElementBuilder<T> {
    this.element.className = classes.join(" ");
    return this;
  }

  style<K extends keyof CSSStyleDeclaration, V extends CSSStyleDeclaration[K]>(
    key: K,
    value: V,
  ): ElementBuilder<T> {
    this.element.style[key] = value;
    return this;
  }

  styleMany(
    style:
      & Partial<
        CSSStyleDeclaration
      >
      & Record<string, string>,
  ): ElementBuilder<T> {
    for (const key in style) {
      if (style[key] !== undefined) {
        // deno-lint-ignore no-explicit-any
        this.element.style[key as any] = style[key];
      }
    }
    return this;
  }

  on<K extends keyof HTMLElementEventMap, E extends HTMLElementEventMap[K]>(
    type: K,
    // deno-lint-ignore no-explicit-any
    listener: (e: E) => any | { handleEvent: (e: E) => any },
    // deno-lint-ignore no-explicit-any
    options?: any,
  ): ElementBuilder<T> {
    this.element.addEventListener(
      type,
      listener as EventListenerOrEventListenerObject,
      options,
    );
    return this;
  }

  with(action: (ref: ElementBuilder<T>) => void): ElementBuilder<T> {
    action(this);
    return this;
  }
}

export function E<
  K extends keyof HTMLElementTagNameMap,
  T extends HTMLElementTagNameMap[K],
>(
  tagNameOrBuilderOrElement: K | ElementBuilder<T> | T,
  options?: ElementCreationOptions,
): ElementBuilder<T> {
  if (tagNameOrBuilderOrElement instanceof ElementBuilder) {
    return tagNameOrBuilderOrElement;
  } else if (tagNameOrBuilderOrElement instanceof HTMLElement) {
    return new ElementBuilder(tagNameOrBuilderOrElement);
  } else {
    return ElementBuilder.fromTagName(tagNameOrBuilderOrElement, options);
  }
}

export function render(
  element: AnyNode,
  target: Element,
) {
  if (element instanceof Array) {
    target.replaceChildren();

    const stack: AnyNode[][] = [[element]];
    while (stack.length > 0) {
      const top = stack.at(-1)!;
      if (top.length === 0) {
        stack.pop();
        continue;
      }

      const node = top.shift()!;
      if (node instanceof ElementBuilder) {
        target.append(node.element);
      } else if (node instanceof Array) {
        stack.push(node);
      } else {
        target.append(node);
      }
    }
  } else if (element instanceof ElementBuilder) {
    target.replaceWith(element.element);
  } else {
    target.replaceWith(element);
  }
}
