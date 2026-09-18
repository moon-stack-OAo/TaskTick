import { getVersion } from "@tauri-apps/api/app";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { invoke } from "@tauri-apps/api/core";

export type UpdateProgress = {
  downloaded: number;
  contentLength: number | null;
};

export async function appVersion(): Promise<string> {
  return getVersion();
}

export async function checkForUpdate(): Promise<Update | null> {
  return check();
}

async function prepareForExit(): Promise<void> {
  await invoke("prepare_for_exit");
}

/** 下载并安装更新；Windows 安装器启动前会先置退出标志。 */
export async function downloadAndInstallUpdate(
  update: Update,
  onProgress?: (p: UpdateProgress) => void,
): Promise<void> {
  let downloaded = 0;
  let contentLength: number | null = null;

  await prepareForExit();

  await update.downloadAndInstall((event) => {
    switch (event.event) {
      case "Started":
        contentLength = event.data.contentLength ?? null;
        onProgress?.({ downloaded, contentLength });
        break;
      case "Progress":
        downloaded += event.data.chunkLength;
        onProgress?.({ downloaded, contentLength });
        break;
      case "Finished":
        onProgress?.({ downloaded, contentLength });
        break;
    }
  });
}

/** 安装完成后重启（非 Windows 安装器路径时有用；Windows 通常已退出）。 */
export async function relaunchApp(): Promise<void> {
  await prepareForExit();
  await relaunch();
}
