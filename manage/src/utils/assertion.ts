export function assertNotNull<T>(t?: T, expect?: string): NonNullable<T> {
  if (t === null || t === undefined) {
    throw new Error(expect ?? `expect value is not null|undefined`);
  }
  return t;
}

export function assert(t: boolean, expect: string): asserts t {
  if (!t) {
    throw new Error(expect);
  }
}

export function unreachable(msg?: string): never {
  throw new Error(msg ?? "unreachable");
}
