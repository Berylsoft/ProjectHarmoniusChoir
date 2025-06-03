import { unreachable } from "./utils/assertion.ts";

export type UserData = {
  user_id: bigint;
  project_id: bigint;
  name: string;
  status: Status;
} & {
  status:
    | Status.PreSubmitPassed
    | Status.Submitted
    | Status.SubmitRejected
    | Status.SubmitPassed
    | Status.Mastered;
  group: GroupInfo;
};

export enum Status {
  Entered,
  PreSubmitted,
  PreSubmitRejected,
  PreSubmitPassed,
  Submitted,
  SubmitRejected,
  SubmitPassed,
  Mastered,
}

export function statusToString(status: Status): string {
  switch (status) {
    case Status.Entered:
      return "已进入";
    case Status.PreSubmitted:
      return "初审已提交";
    case Status.PreSubmitRejected:
      return "初审未通过";
    case Status.PreSubmitPassed:
      return "初审已通过";
    case Status.Submitted:
      return "已提交";
    case Status.SubmitRejected:
      return "未通过";
    case Status.SubmitPassed:
      return "已通过";
    case Status.Mastered:
      return "已修对";
    default:
      unreachable();
  }
}

export type GroupInfo = {
  lead: boolean;
  choir: boolean;
  harmony: boolean;
};

export enum SubmitStatus {
  None,
  Passed,
  Rejected,
}

export function submitStatusToString(status: SubmitStatus): string {
  switch (status) {
    case SubmitStatus.None:
      return "未处理";
    case SubmitStatus.Passed:
      return "已通过";
    case SubmitStatus.Rejected:
      return "未通过";
    default:
      unreachable();
  }
}

export type FileInfo = {
  id: bigint;
  name: string;
  url: string;
  checked?: boolean;
};

export enum RejectReason {
  DeviceOrEnvironment,
  RequirementNotMet,
  Other,
}

export function rejectReasonToString(reason: RejectReason) {
  switch (reason) {
    case RejectReason.DeviceOrEnvironment:
      return "设备或环境";
    case RejectReason.RequirementNotMet:
      return "未达标";
    default:
      unreachable();
  }
}

export type PreSubmitInfo = {
  id: bigint;
  user_id: bigint;
  project_id: bigint;
  // seconds
  time: number;
  file: FileInfo;
  harmonyGroupIntention?: boolean;
  status: SubmitStatus;
  // only passed
  group?: GroupInfo;
  // only rejected
  reason?: RejectReason;
};

export type SubmitInfo = {
  id: bigint;
  user_id: bigint;
  project_id: bigint;
  // seconds
  time: number;
  files: FileInfo[];
  comment: string;
  status: SubmitStatus;
  // only rejected
  reason?: RejectReason;
  // only rejected (optional)
  reasonDetail?: string;
};

export type MasterInfo = {
  user_id: bigint;
  project_id: bigint;
  // seconds
  time: number;
  file: FileInfo;
  comment: string;
};
