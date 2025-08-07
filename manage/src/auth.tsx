import { acquireSudo, login, LoginRes } from "./api.ts";
import { ElementBuilder, render } from "./libs/ElementBuilder.ts";
import { createSignal, Signal } from "./libs/Signal.ts";
import { notify } from "./notify.ts";
import { assert, assertNotNull, unreachable } from "./utils/assertion.ts";
import { debug, trace } from "./utils/logging.ts";
import { QRCode } from "./libs/qrcode.js";
import { writeNav } from "./nav.ts";

export function Auth(props: { nav: URL }) {
  const ref = createSignal<null | ElementBuilder<HTMLDivElement>>(null);

  function onSuccess() {
    props.nav.host = "manage";
    props.nav.search = "";
    writeNav(props.nav, true);
  }

  let auth;
  if (props.nav.searchParams.get("sudo")) {
    auth = <AcquireSudo success={onSuccess} />;
  } else {
    auth = (
      <LoginStart
        continueLogin={(res) => {
          assert(
            res != "InvalidCredential" && res != "Success",
            "expect InvalidCredential handled inside LoginStart and need totp verify",
          );

          const auth = assertNotNull(ref.get(), "expect ref of #auth");

          function onFailed() {
            render(<Auth {...props} />, auth.element);
          }

          if ("TotpSetup" in res) {
            auth.children(
              <TotpSetup
                res={res}
                failed={onFailed}
                success={onSuccess}
              />,
            );
          } else {
            const auth = assertNotNull(ref.get(), "expect ref of #auth");
            auth.children(
              <TotpVerify
                res={res}
                failed={onFailed}
                success={onSuccess}
              />,
            );
          }
        }}
      />
    );
  }

  return (
    <div
      id="auth"
      with={(it) => {
        ref.set(it);
      }}
    >
      {auth}
    </div>
  );
}

export function LoginStart(props: { continueLogin: (res: LoginRes) => void }) {
  const mid = createSignal(0);
  const passwd = createSignal("");
  const waiting = createSignal(false);

  return (
    <div class="authContainer">
      <form
        id="authPanel"
        on:submit={(e) => {
          e.preventDefault();
          const form = e.target! as HTMLFormElement;
          if (!form.checkValidity()) {
            form.reportValidity();
            return;
          }

          waiting.set(true);
          (async () => {
            const encoder = new TextEncoder();
            const passwdUtf8 = encoder.encode(passwd.get());
            const passwdHash = await crypto.subtle.digest(
              "SHA-512",
              passwdUtf8,
            );

            const res = await login({
              Start: {
                mid: mid.get(),
                password: new Uint8Array(passwdHash),
              },
            });
            if (res === "InvalidCredential") {
              notify("MID或密码错误");
            } else {
              props.continueLogin(res);
            }
          })().then(() => {
            waiting.set(false);
          });
        }}
      >
        <div id="loginInputs">
          <div class="authInput">
            <label htmlFor="midInput">MID</label>
            <input
              type="number"
              id="midInput"
              step="1"
              min="0"
              max={Number.MAX_SAFE_INTEGER.toString()}
              autocomplete="username"
              required
              on:change={(e) => {
                const id = (e.target! as HTMLInputElement).valueAsNumber;
                if (Number.isSafeInteger(id) && id >= 0) {
                  trace("mid %o", id);
                  mid.set(id);
                } else {
                  debug("invalid integer %o", id);
                }
              }}
            />
          </div>
          <div class="authInput">
            <label htmlFor="passwdInput">密码</label>
            <input
              type="password"
              id="passwdInput"
              minLength={1}
              autocomplete="current-password"
              required
              on:change={(e) => {
                passwd.set((e.target! as HTMLInputElement).value);
              }}
            />
          </div>
        </div>
        <button type="submit" class="authBtn" sub:disabled={waiting}>
          登录
        </button>
      </form>
    </div>
  );
}

export function TotpSetup(
  props: { res: LoginRes; failed: () => void; success: () => void },
) {
  assert(
    typeof props.res === "object" && "TotpSetup" in props.res,
    "expect only TotpSetup",
  );
  const setup = props.res.TotpSetup;

  const totpCode = createSignal("");
  const waiting = createSignal(false);

  const secret = new URL(setup.totp_url).searchParams.get(
    "secret",
  )!;

  return (
    <div class="authContainer">
      <form
        id="authTotpSetupPanel"
        on:submit={(e) => {
          e.preventDefault();
          const form = e.target! as HTMLFormElement;
          if (!form.checkValidity()) {
            form.reportValidity();
            return;
          }

          waiting.set(true);
          (async () => {
            const res = await login({
              EndSetup: {
                token: setup.token,
                totp_code: Number.parseInt(totpCode.get()),
              },
            });
            if (res === "InvalidCredential") {
              notify("错误的验证码");
              props.failed();
            } else if (res === "Success") {
              props.success();
            } else {
              unreachable();
            }
          })().then(() => {
            waiting.set(false);
          });
        }}
      >
        <div id="authTotpSetupPanelLeft">
          <div class="authInput">
            <div>使用验证器扫描此二维码:</div>
            <div
              id="authTotpSetupQrCode"
              with={(it) => {
                new QRCode(it.element, setup.totp_url);
              }}
            />
          </div>
          <div class="authInput">
            <div>或使用密钥:</div>
            <div id="authTotpSetupSecret">
              {secret}
            </div>
          </div>
        </div>
        <div id="authTotpSetupPanelRight">
          <TotpCodeInput totpCode={totpCode} />
          <button type="submit" class="authBtn" sub:disabled={waiting}>
            完成设置
          </button>
        </div>
      </form>
    </div>
  );
}

export function TotpVerify(
  props: { res: LoginRes; failed: () => void; success: () => void },
) {
  assert(
    typeof props.res === "object" && "TotpVerify" in props.res,
    "expect only TotpVerify",
  );
  const verify = props.res.TotpVerify;

  const totpCode = createSignal("");
  const waiting = createSignal(false);

  return (
    <div class="authContainer">
      <form
        id="authPanel"
        on:submit={(e) => {
          e.preventDefault();
          const form = e.target! as HTMLFormElement;
          if (!form.checkValidity()) {
            form.reportValidity();
            return;
          }

          waiting.set(true);
          (async () => {
            const res = await login({
              End: {
                token: verify.token,
                totp_code: Number.parseInt(totpCode.get()),
              },
            });
            if (res === "InvalidCredential") {
              notify("错误的验证码");
              props.failed();
            } else if (res === "Success") {
              props.success();
            } else {
              unreachable();
            }
          })().then(() => {
            waiting.set(false);
          });
        }}
      >
        <TotpCodeInput totpCode={totpCode} />
        <button type="submit" class="authBtn" sub:disabled={waiting}>
          验证
        </button>
      </form>
    </div>
  );
}

export function AcquireSudo(
  props: { success: () => void },
) {
  const totpCode = createSignal("");
  const waiting = createSignal(false);

  return (
    <div class="authContainer">
      <form
        id="authPanel"
        on:submit={(e) => {
          e.preventDefault();
          const form = e.target! as HTMLFormElement;
          if (!form.checkValidity()) {
            form.reportValidity();
            return;
          }

          waiting.set(true);
          (async () => {
            const res = await acquireSudo({
              totp_code: Number.parseInt(totpCode.get()),
            });
            if (res === "InvalidCredential") {
              notify("错误的验证码");
            } else if (res === "Success") {
              props.success();
            } else {
              unreachable();
            }
          })().then(() => {
            waiting.set(false);
          });
        }}
      >
        <div id="authAcquireSudoDesc">
          你正在进行高危操作，我们需要验证你的身份
        </div>
        <div></div>
        <TotpCodeInput totpCode={totpCode} />
        <button type="submit" class="authBtn" sub:disabled={waiting}>
          验证
        </button>
      </form>
    </div>
  );
}

function TotpCodeInput(props: { totpCode: Signal<string> }) {
  return (
    <div class="authInput">
      <label htmlFor="totpCodeInput">验证码</label>
      <input
        type="text"
        id="totpCodeInput"
        inputMode="numric"
        pattern="\d{6}"
        minLength={6}
        maxLength={6}
        autocomplete="one-time-code"
        required
        on:change={(e) => {
          const input = e.target! as HTMLInputElement;
          const code = Number.parseInt(
            input.value,
          );

          if (Number.isSafeInteger(code) && code >= 0 && code <= 999999) {
            trace("code %o", code);
            props.totpCode.set(code.toString().padStart(6, "0"));
            input.setCustomValidity("");
          } else {
            debug("invalid code %o", code);
            input.setCustomValidity("请输入正确的验证码");
          }
        }}
      />
    </div>
  );
}
