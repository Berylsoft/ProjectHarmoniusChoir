export let redirectAuth: () => never = () => {
  throw new Error("uninit");
};

export function setRedirectAuth(f: () => never) {
  redirectAuth = f;
}

export let redirectAuthSudo: () => never = () => {
  throw new Error("uninit");
};

export function setRedirectAuthSudo(f: () => never) {
  redirectAuthSudo = f;
}
