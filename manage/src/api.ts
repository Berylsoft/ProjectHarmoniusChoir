import CBOR from "./libs/cbor.js";
import { ulid } from "jsr:@std/ulid";
import { redirectAuth, redirectAuthSudo } from "./redirect.ts";
import { notify } from "./notify.ts";

const ENDPOINT: string = "http://localhost";

type Path =
  | "/login"
  | "/acquire_sudo"
  | "/get_info"
  | "/list_projects"
  | "/list_project_users"
  | "/pre_submit_info"
  | "/get_file"
  | "/pre_submit_review"
  | "/submit_info"
  | "/submit_review"
  | "/upload_file"
  | "/list_pending_files"
  | "/delete_file"
  | "/master"
  | "/master_info"
  | "/bundle_job"
  | "/root/create_project"
  | "/root/create_manager"
  | "/root/list_managers"
  | "/root/project_manager_edit";

type OpaqueType = { __opaque__: undefined };

type Data = {
  [key: string]: string | number | bigint | Uint8Array | OpaqueType | Data;
};

export async function post<T>(path: Path, body?: Data): Promise<T> {
  const bodyEncoded = CBOR.encode({ data: body, nonce: ulid() });

  const res = await fetch(`${ENDPOINT}/api/manager${path}`, {
    method: "POST",
    headers: {
      "Content-Type": "application/cbor",
    },
    body: bodyEncoded,
  });

  if (res.status == 401) {
    redirectAuth();
  } else if (res.status != 200) {
    // TODO: ui popup
    if (res.headers.get("Content-Type") === "application/cbor") {
      const data = await res.arrayBuffer();
      const dataCbor = CBOR.decode(data);

      if (res.status === 403 && dataCbor?.Err?.code === 4004) {
        redirectAuthSudo();
      }

      const dataStr = JSON.stringify(dataCbor);
      notify(`未知错误, 请反馈 ${res.status} ${dataStr}`);
      throw new Error();
    } else {
      const data = await res.text();
      notify(`未知错误, 请反馈 ${res.status} ${data}`);
      throw new Error(data);
    }
  }

  const data = await res.arrayBuffer();
  return CBOR.decode(data)["Ok"];
}

export type LoginTotpSetupToken = OpaqueType;
export type LoginToken = OpaqueType;
export type LoginReq =
  | { Start: { mid: number; password: Uint8Array } }
  | { EndSetup: { token: LoginTotpSetupToken; totp_code: number } }
  | { End: { token: LoginToken; totp_code: number } };
export type LoginRes =
  | { TotpSetup: { token: LoginTotpSetupToken; totp_url: string } }
  | { TotpVerify: { token: LoginToken } }
  | "Success"
  | "InvalidCredential";
export async function login(req: LoginReq): Promise<LoginRes> {
  return await post("/login", req);
}

export type AcquireSudoReq = { totp_code: number };
export type AcquireSudoRes = "Success" | "InvalidCredential";
export async function acquireSudo(
  req: AcquireSudoReq,
): Promise<AcquireSudoRes> {
  return await post("/acquire_sudo", req);
}

