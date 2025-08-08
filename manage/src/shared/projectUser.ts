import { unreachable } from "../utils/assertion.ts";

export type Status =
  | "Entered"
  | "PreSubmitted"
  | "PreSubmitRejected"
  | "PreSubmitPassed"
  | "Submitted"
  | "SubmitRejected"
  | "SubmitPassed"
  | "Mastered"
  | "Mixed";

export function statusToStageName(status: Status): string {
  switch (status) {
    case "Entered":
      return "加入";
    case "PreSubmitted":
    case "PreSubmitRejected":
    case "PreSubmitPassed":
      return "初审";
    case "Submitted":
    case "SubmitRejected":
    case "SubmitPassed":
      return "正式";
    case "Mastered":
      return "修对";
    case "Mixed":
      return "混音";
    default:
      unreachable();
  }
}

export enum StatusEnum {
  Entered,
  PreSubmitted,
  PreSubmitRejected,
  PreSubmitPassed,
  Submitted,
  SubmitRejected,
  SubmitPassed,
  Mastered,
  Mixed,
}

export function statusToStatusEnum(status: Status): StatusEnum {
  switch (status) {
    case "Entered":
      return StatusEnum.Entered;
    case "PreSubmitted":
      return StatusEnum.PreSubmitted;
    case "PreSubmitRejected":
      return StatusEnum.PreSubmitRejected;
    case "PreSubmitPassed":
      return StatusEnum.PreSubmitPassed;
    case "Submitted":
      return StatusEnum.Submitted;
    case "SubmitRejected":
      return StatusEnum.SubmitRejected;
    case "SubmitPassed":
      return StatusEnum.SubmitPassed;
    case "Mastered":
      return StatusEnum.Mastered;
    case "Mixed":
      return StatusEnum.Mixed;
    default:
      unreachable();
  }
}
