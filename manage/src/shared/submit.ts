export type GroupInfo = {
  lead: boolean;
  choir: boolean;
  harmony: boolean;
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
