export type OpMode = "slient" | "force" | "default";

export type Comparator<T> = (a: T, b: T) => boolean;

export type SubscriberFn<T> = (newValue: T) => void;

export type Subscriber<T> = SubscriberFn<T> | WeakRef<SubscriberFn<T>>;

export class Signal<T> {
  value: T;
  isEq: Comparator<T> = (a, b) => a === b;
  subscribers: Subscriber<T>[] = [];

  constructor(defaultValue: T) {
    this.value = defaultValue;
  }

  set(newValue: T, mode?: OpMode) {
    const eq = this.isEq(this.value, newValue);
    this.value = newValue;
    if (mode === "slient") return;
    if (mode === "force" || !eq) {
      this.notify();
    }
  }

  get(): T {
    return this.value;
  }

  update(f: (previousValue: T) => T, mode?: OpMode) {
    const oldValue = this.value;
    this.value = f(this.value);
    if (mode === "slient") return;
    if (mode === "force" || !this.isEq(oldValue, this.value)) {
      this.notify();
    }
  }

  setComparator(isEq: Comparator<T>) {
    this.isEq = isEq;
  }

  subscribe(subscriber: Subscriber<T>): () => void {
    this.subscribers.push(subscriber);
    return this.unsubscribe.bind(this, subscriber);
  }

  unsubscribe(subscriber: Subscriber<T>) {
    this.subscribers = this.subscribers.filter(
      (it) => it !== subscriber,
    );
  }

  notify() {
    let idx = 0;
    while (idx < this.subscribers.length) {
      const it = this.subscribers[idx];
      if (it instanceof WeakRef) {
        const fn = it.deref();
        if (fn) {
          fn(this.value);
          idx += 1;
        } else {
          this.subscribers.splice(idx, 1);
        }
      } else {
        it(this.value);
        idx += 1;
      }
    }
  }
}

export function createSignal<T>(defaultValue: T): Signal<T> {
  return new Signal(defaultValue);
}

/**
 * subscribe to changes in deps
 * return the subscribers and a unsubscribe function
 * when weak=true drop the subscribers will unsubscribe
 */
export function createEffect<
  // deno-lint-ignore no-explicit-any
  Deps extends Record<string, Signal<any>>,
  Values extends {
    [P in keyof Deps]: Deps[P] extends Signal<infer ValueType> ? ValueType
      : never;
  },
>(
  deps: Deps,
  onChange: (values: Readonly<Values>) => (() => void) | void,
  weak?: boolean,
): [
  {
    [K in keyof Deps]: [
      SubscriberFn<Values[K]>,
      WeakRef<SubscriberFn<Values[K]>>,
    ];
  },
  () => void,
] {
  let onCancel: (() => void) | void = undefined;
  const deps_cache = Object.fromEntries(
    Object.entries(deps).map(([k, v]) => [k, v.get()]),
  ) as Values;

  const notify = <K extends keyof Deps>(key: K, value: Values[K]) => {
    if (onCancel) {
      onCancel();
    }

    deps_cache[key] = value;
    onCancel = onChange(Object.freeze({ ...deps_cache }));
  };

  const subscribers: {
    [K in keyof Deps]: [
      SubscriberFn<Values[K]>,
      WeakRef<SubscriberFn<Values[K]>>,
    ];
  } = Object.fromEntries(
    Object.keys(deps).map((it: keyof Deps) => {
      const sub = notify.bind(null, it);
      return [it, [sub, new WeakRef(sub)]];
    }),
    // deno-lint-ignore no-explicit-any
  ) as any;

  for (const [key, sub] of Object.entries(subscribers)) {
    deps[key].subscribe(sub[weak ? 1 : 0]);
  }

  return [subscribers, () => {
    for (const [key, sub] of Object.entries(subscribers)) {
      deps[key].unsubscribe(sub[weak ? 1 : 0]);
    }
  }];
}
