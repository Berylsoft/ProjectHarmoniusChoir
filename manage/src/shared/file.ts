export type Info = {
  id: number;
  name: string;
};

export type PresignedReq = {
  method: string;
  uri: string;
  headers: [string, string][];
};
