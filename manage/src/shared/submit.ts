export type GroupInfo = {
  lead: boolean;
  choir: boolean;
  harmony: boolean;
};

export const groupInfoTxt: {
  [K in keyof GroupInfo]: string;
} = {
  lead: "领唱",
  choir: "合唱",
  harmony: "和声",
};

export type Detail<S> = {
  mid: number;
  mname: string;
  status: S;
};

export type PreSubmitStatus =
  | { "Rejected": { reason: PreSubmitRejectReason } }
  | { "Passed": GroupInfo };

export type PreSubmitRejectReason =
  | "DeviceOrEnvironment"
  | "RequirementNotMet"
  | "InvalidName";

export const preSubmitRejectReasonTxt: {
  [T in PreSubmitRejectReason]: string;
} = {
  DeviceOrEnvironment: "设备或环境",
  RequirementNotMet: "未达到标准",
  InvalidName: "用户名问题",
};
