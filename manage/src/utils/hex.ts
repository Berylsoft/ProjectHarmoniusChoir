export function uint8arrayToHex(data: Uint8Array): string {
  let result = "";
  data.forEach((it) => {
    result += it.toString(16).padStart(2, "0");
  });
  return result;
}

