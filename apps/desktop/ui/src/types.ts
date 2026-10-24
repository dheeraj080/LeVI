export interface Header {
  key: string;
  value: string;
  enabled: boolean;
}

export interface Auth {
  type: "none" | "bearer" | "basic" | "apikey";
  bearerToken?: string;
  basicUsername?: string;
  basicPassword?: string;
  apiKeyName?: string;
  apiKeyValue?: string;
  apiKeyIn?: "header" | "query";
}

export interface RequestTab {
  id: string;
  name: string;
  method: string;
  url: string;
  headers: Header[];
  bodyType: "none" | "json" | "raw" | "form" | "graphql";
  bodyContent: string;
  auth: Auth;
  response?: ResponseData;
}

export interface ResponseData {
  status: number;
  headers: [string, string][];
  body: string;
  timings: any;
}
