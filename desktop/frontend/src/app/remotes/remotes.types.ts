/**
 * Type definitions for the remotes component
 */

export type RemoteType =
    | "drive"
    | "dropbox"
    | "onedrive"
    | "yandex"
    | "gphotos"
    | "iclouddrive";

export interface RemoteFormData {
    name: string;
    type: RemoteType;
}

export interface RemoteTypeOption {
    value: RemoteType;
    label: string;
    icon: string;
}

export interface ConfirmDeleteDialogData {
    remoteName: string;
}

export interface RemoteInfo {
    name: string;
    type: string;
    source?: string;
    description?: string;
}

// Constants for remote type options
export const REMOTE_TYPE_OPTIONS: RemoteTypeOption[] = [
    { value: "drive", label: "Google Drive", icon: "googledrive" },
    { value: "dropbox", label: "Dropbox", icon: "dropbox" },
    { value: "onedrive", label: "OneDrive", icon: "microsoftonedrive" },
    { value: "yandex", label: "Yandex Disk", icon: "yandex" },
    { value: "gphotos", label: "Google Photos", icon: "googlephotos" },
    { value: "iclouddrive", label: "iCloud Drive", icon: "icloud" },
];

// Type guards
export function isValidRemoteType(type: string): type is RemoteType {
    return [
        "drive",
        "dropbox",
        "onedrive",
        "yandex",
        "gphotos",
        "iclouddrive",
    ].includes(type);
}

export function isRemoteFormData(data: unknown): data is RemoteFormData {
    return (
        data !== null &&
        typeof data === "object" &&
        data !== undefined &&
        "name" in data &&
        "type" in data &&
        typeof (data as RemoteFormData).name === "string" &&
        (data as RemoteFormData).name.length > 0 &&
        typeof (data as RemoteFormData).type === "string" &&
        isValidRemoteType((data as RemoteFormData).type)
    );
}
