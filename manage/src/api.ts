import CBOR from "./libs/cbor.js";
import { ulid } from "jsr:@std/ulid";
import { redirectAuth, redirectAuthSudo } from "./redirect.ts";
import { notify } from "./notify.ts";
import { Status } from "./shared/projectUser.ts";
import {
  Detail as SubmitDetail,
  GroupInfo,
  PreSubmitStatus,
  SubmitStatus,
} from "./shared/submit.ts";
import { Info as FileInfo, PresignedReq } from "./shared/file.ts";
import { assert } from "./utils/assertion.ts";

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

type DataPrimitives =
  | null
  | boolean
  | number
  | string
  | bigint
  | Uint8Array
  | OpaqueType;

type Data = {
  [key: string]:
    | DataPrimitives
    | DataPrimitives[]
    | Data;
};

export async function post<T>(path: Path, body?: Data): Promise<T> {
  const bodyEncoded = CBOR.encode({ data: body, nonce: ulid() });

  const res = await fetch(`${ENDPOINT}/api/manager${path}`, {
    method: "POST",
    headers: {
      "Content-Type": "application/cbor",
    },
    body: new Uint8Array(bodyEncoded),
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

(globalThis as Record<string, unknown>)["apiPost"] = post;

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

export type GetInfoRes = { id: number };
export async function getInfo(): Promise<GetInfoRes> {
  return await post("/get_info");
}

export type ListProjectsRes = { projects: ListProjectsProject[] };
export type ListProjectsProject = {
  pid: number;
  name: string;
};
export async function listProjects(): Promise<ListProjectsRes> {
  return await post("/list_projects");
}

export type ListProjectUsersReq = {
  pid: number;
  sort_by?: "JoinedAt" | "Status";
  reverse?: boolean;
};
export type ListProjectUsersRes = {
  project_users: ListProjectUsersProjectUser[];
};
export type ListProjectUsersProjectUser = {
  id: number;
  uid: number;
  status: Status;
  name: null | string;
  group_info: null | GroupInfo;
};
export async function listProjectUsers(
  req: ListProjectUsersReq,
): Promise<ListProjectUsersRes> {
  return await post("/list_project_users", req);
}

export type PreSubmitInfoReq = {
  puid: number;
};
export type PreSubmitInfoRes = {
  pre_submits: PreSubmitInfo[];
};
export type PreSubmitInfo = {
  id: number;
  created_at: string;
  name: string;
  harmony_group_intention: null | boolean;
  comment: string;
  file_info: null | FileInfo;
  status: null | SubmitDetail<PreSubmitStatus>;
};
export async function preSubmitInfo(
  req: PreSubmitInfoReq,
): Promise<PreSubmitInfoRes> {
  return await post("/pre_submit_info", req);
}

export type GetFileReq = {
  pid: number;
  file_id: number;
  type: GetFileType;
};
export type GetFileType = "Preview" | "Download";
export type GetFileRes = {
  presigned_req: PresignedReq;
};
export async function getFile(req: GetFileReq): Promise<GetFileRes> {
  return await post("/get_file", req);
}
export async function openFile(pid: number, id: number, type: GetFileType) {
  const req = (await getFile({ pid, file_id: id, type })).presigned_req;
  assert(req.method === "GET", "expect get");
  assert(req.headers.length === 0, "expect no header");

  globalThis.open(req.uri);
}

export type PreSubmitReviewReq = {
  pid: number;
  sid: number;
  status: PreSubmitStatus;
};
export async function preSubmitReview(req: PreSubmitReviewReq) {
  await post("/pre_submit_review", req);
}

export type SubmitInfoReq = {
  puid: number;
};
export type SubmitInfoRes = {
  submits: SubmitInfo[];
  checked_files: number[];
};
export type SubmitInfo = {
  id: number;
  created_at: string;
  comment: string;
  files: FileInfo[];
  status: null | SubmitDetail<SubmitStatus>;
};
export async function submitInfo(
  req: SubmitInfoReq,
): Promise<SubmitInfoRes> {
  return await post("/submit_info", req);
}

export type SubmitReviewReq = {
  pid: number;
  sid: number;
  status: SubmitStatus;
  checked_files: null | number[];
};
export async function submitReview(req: SubmitReviewReq) {
  await post("/submit_review", req);
}
