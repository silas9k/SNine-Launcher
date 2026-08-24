import { invoke } from "@tauri-apps/api/core";

export type CapeStatus = "PENDING" | "APPROVED" | "REJECTED";
export type CapeTemplate = "CAPE" | "CAPE_ELYTRA";

export interface CustomCapeView {
  id: string;
  ownerUuid: string;
  ownerName: string;
  capeName: string;
  template: CapeTemplate;
  status: CapeStatus;
  submittedAt: number;
  reviewedAt: number;
  reviewedBy: string;
  rejectionReason: string;
  uses: number;
  favorite: boolean;
  selected: boolean;
  textureEndpoint: string;
  textureDataUrl?: string | null;
}

export interface CustomCapeListResponse {
  ok: boolean;
  capes: CustomCapeView[];
  selected: CustomCapeView | null;
}

export interface VanillaCapeView {
  id: string;
  name: string;
  state: string;
  textureDataUrl: string | null;
}

export interface VanillaCapeResponse {
  ok: boolean;
  playerName: string;
  capes: VanillaCapeView[];
}

interface AccountRef { accountId: string; username: string }

export const capeCommands = {
  list: ({ accountId, username }: AccountRef, scope: string, search: string): Promise<CustomCapeListResponse> =>
    invoke<CustomCapeListResponse>("snine_launcher_custom_capes", { accountId, username, scope, search }),
  upload: ({ accountId, username }: AccountRef, capeName: string, template: CapeTemplate, imageBase64: string): Promise<{ ok: boolean; cape: CustomCapeView }> =>
    invoke("snine_launcher_custom_cape_upload", { accountId, username, capeName, template, imageBase64 }),
  favorite: ({ accountId, username }: AccountRef, capeId: string, favorite: boolean): Promise<{ ok: boolean; cape: CustomCapeView }> =>
    invoke("snine_launcher_custom_cape_favorite", { accountId, username, capeId, favorite }),
  equip: ({ accountId, username }: AccountRef, capeId: string): Promise<{ ok: boolean; cape: CustomCapeView }> =>
    invoke("snine_launcher_custom_cape_equip", { accountId, username, capeId }),
  unequip: ({ accountId, username }: AccountRef): Promise<{ ok: boolean }> =>
    invoke("snine_launcher_custom_cape_unequip", { accountId, username }),
  texture: (capeId: string): Promise<string> =>
    invoke<string>("snine_launcher_custom_cape_texture", { capeId }),
  preview: ({ accountId, username }: AccountRef, capeId: string): Promise<string> =>
    invoke<string>("snine_launcher_custom_cape_preview", { accountId, username, capeId }),
  saveTemplate: (): Promise<string> =>
    invoke<string>("snine_launcher_save_cape_template"),
  vanilla: (accountId: string): Promise<VanillaCapeResponse> =>
    invoke<VanillaCapeResponse>("snine_launcher_vanilla_capes", { accountId }),
};
