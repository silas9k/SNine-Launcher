export type AccountKind = "microsoft";

export interface Account {
  id: string;
  username: string;
  kind: AccountKind;
  sessionState: "active" | "relogin-required";
  ownershipVerifiedAtUnix: number;
  lastOnlineAuthAtUnix: number;
  addedAtUnix: number;
  lastUsedAtUnix: number;
}

export type BackgroundStyle = "void" | "carbon" | "aurora" | "ember" | "glacier" | "nebula";
export type UiDensity = "compact" | "comfortable";
export type PanelStyle = "glass" | "solid" | "outline";
export type CornerStyle = "sharp" | "soft" | "round";
export type SkinPose = "hero" | "relaxed" | "classic";

export interface LauncherSettings {
  ultimate_installer_mode: boolean;
  game_version: string;
  memory_mb: number;
  java_path: string | null;
  game_directory: string;
  close_on_launch: boolean;
  accent_color: string;
  background_style: BackgroundStyle;
  ui_density: UiDensity;
  reduced_motion: boolean;
  glow_intensity: number;
  panel_style: PanelStyle;
  corner_style: CornerStyle;
  sidebar_labels: boolean;
  skin_scale: number;
  skin_pose: SkinPose;
  skin_animation: boolean;
  secondary_accent: string;
  surface_opacity: number;
  background_motion: boolean;
}

export interface ClientStatus {
  installed: boolean;
  game_version: string;
  fabric_loader: string | null;
  java_found: boolean;
  java_path: string | null;
  bundled_mods: string[];
}

export type LaunchState = "idle" | "starting" | "running" | "stopping" | "failed";

export interface LaunchStatus {
  state: LaunchState;
  process_id: number | null;
  account_name: string | null;
  started_at_unix: number | null;
  message: string | null;
  running_instances: number;
}

export interface LauncherSnapshot {
  accounts: Account[];
  activeAccountId: string | null;
  settings: LauncherSettings;
  client: ClientStatus;
  launch: LaunchStatus;
}

export interface InstallProgress {
  stage: string;
  detail: string;
  current: number;
  total: number;
  percent: number;
}

export interface LogEvent {
  stream: "system" | "stdout" | "stderr";
  line: string;
  timestamp_unix: number;
}
