export type PresignedReq = {
  method: string;
  uri: string;
  headers: [string, string][];
};
