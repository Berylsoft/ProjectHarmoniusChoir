// ElementBuilder.ts
declare type AnyNode =
  | Node
  | string
  | ElementBuilder<HTMLElement>
  | AnyNode[];

declare class ElementBuilder<T extends HTMLElement> {
  element: T;

  constructor(element: T);

  static fromTagName<
    K extends keyof HTMLElementTagNameMap,
    T extends HTMLElementTagNameMap[K],
  >(tagName: K, options?: ElementCreationOptions): ElementBuilder<T>;

  children(
    ...nodes: AnyNode[]
  ): ElementBuilder<T>;

  attr(name: string, value?: string): ElementBuilder<T>;

  data(name: string, value: string, remove?: boolean): ElementBuilder<T>;

  id(id: string): ElementBuilder<T>;

  classes(...classes: string[]): ElementBuilder<T>;

  style<K extends keyof CSSStyleDeclaration, V extends CSSStyleDeclaration[K]>(
    key: K,
    value: V,
  ): ElementBuilder<T>;

  styleMany(
    style:
      & Partial<
        CSSStyleDeclaration
      >
      & Record<string, string>,
  ): ElementBuilder<T>;

  on<K extends keyof HTMLElementEventMap, E extends HTMLElementEventMap[K]>(
    type: K,
    // deno-lint-ignore no-explicit-any
    listener: (e: E) => any | { handleEvent: (e: E) => any },
    // deno-lint-ignore no-explicit-any
    options?: any,
  ): ElementBuilder<T>;

  with(action: (ref: ElementBuilder<T>) => void): ElementBuilder<T>;
}

// Signal.ts
type OpMode = "slient" | "force" | "default";

type Comparator<T> = (a: T, b: T) => boolean;

type SubscriberFn<T> = (newValue: T) => void;

type Subscriber<T> = SubscriberFn<T> | WeakRef<SubscriberFn<T>>;

declare class Signal<T> {
  value: T;
  isEq: Comparator<T>;
  subscribers: Subscriber<T>[];

  constructor(defaultValue: T);

  set(newValue: T, mode?: OpMode | undefined): void;

  get(): T;

  update(f: (previousValue: T) => T, mode?: OpMode | undefined): void;

  setComparator(isEq: Comparator<T>): void;

  subscribe(subscriber: Subscriber<T>): () => void;

  unsubscribe(subscriber: Subscriber<T>): void;

  notify(): void;
}

// jsx
declare type WithListeners = {
  [K in keyof HTMLElementEventMap as `on:${K & string}`]?: (
    event: HTMLElementEventMap[K],
  ) => void;
};

declare type WithSubscribers<E extends HTMLElement> = {
  [K in keyof E as `sub:${K & string}`]?: Signal<E[K]>;
};

type HTMLElementAttrs<K extends keyof HTMLElementTagNameMap> = Omit<
  Omit<
    Partial<HTMLElementTagNameMap[K]>,
    "children"
  >,
  "style"
>;

type ElementStyle = Partial<
  {
    [K in keyof CSSStyleDeclaration]: CSSStyleDeclaration[K];
  }
>;

declare namespace JSX {
  type Element = AnyNode;
  type IntrinsicElements = {
    [K in keyof HTMLElementTagNameMap]:
      & HTMLElementAttrs<K>
      & {
        children?: AnyNode;
        class?: string;
        classes?: string[];
        style?: ElementStyle;
        with?: (ref: ElementBuilder<HTMLElementTagNameMap[K]>) => void;
      }
      & WithListeners
      & WithSubscribers<HTMLElementTagNameMap[K]>;
  };
}
