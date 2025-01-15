export namespace config {
	
	export class Config {
	    DebugMode: boolean;
	    ProfileFilePath: string;
	    ResyncFilePath: string;
	    RcloneFilePath: string;
	
	    static createFrom(source: any = {}) {
	        return new Config(source);
	    }
	
	    constructor(source: any = {}) {
	        if ('string' === typeof source) source = JSON.parse(source);
	        this.DebugMode = source["DebugMode"];
	        this.ProfileFilePath = source["ProfileFilePath"];
	        this.ResyncFilePath = source["ResyncFilePath"];
	        this.RcloneFilePath = source["RcloneFilePath"];
	    }
	}
	export class Remote {
	    name: string;
	    type: string;
	    source: string;
	    description: string;
	
	    static createFrom(source: any = {}) {
	        return new Remote(source);
	    }
	
	    constructor(source: any = {}) {
	        if ('string' === typeof source) source = JSON.parse(source);
	        this.name = source["name"];
	        this.type = source["type"];
	        this.source = source["source"];
	        this.description = source["description"];
	    }
	}

}

export namespace constants {
	
	export enum Platform {
	    windows = 0,
	    darwin = 1,
	    linux = 2,
	}
	export enum Environment {
	    development = 0,
	    production = 1,
	}

}

export namespace dto {
	
	export enum Command {
	    command_stoped = "command_stoped",
	    command_output = "command_output",
	    command_started = "command_started",
	    working_dir_updated = "working_dir_updated",
	    error = "error",
	}
	export class AppError {
	    message: string;
	
	    static createFrom(source: any = {}) {
	        return new AppError(source);
	    }
	
	    constructor(source: any = {}) {
	        if ('string' === typeof source) source = JSON.parse(source);
	        this.message = source["message"];
	    }
	}

}

export namespace models {
	
	export class Profile {
	    name: string;
	    from: string;
	    to: string;
	    included_paths: string[];
	    excluded_paths: string[];
	    bandwidth: number;
	    parallel: number;
	    backup_path: string;
	    cache_path: string;
	
	    static createFrom(source: any = {}) {
	        return new Profile(source);
	    }
	
	    constructor(source: any = {}) {
	        if ('string' === typeof source) source = JSON.parse(source);
	        this.name = source["name"];
	        this.from = source["from"];
	        this.to = source["to"];
	        this.included_paths = source["included_paths"];
	        this.excluded_paths = source["excluded_paths"];
	        this.bandwidth = source["bandwidth"];
	        this.parallel = source["parallel"];
	        this.backup_path = source["backup_path"];
	        this.cache_path = source["cache_path"];
	    }
	}
	export class ConfigInfo {
	    working_dir: string;
	    selected_profile_index: number;
	    profiles: Profile[];
	    env_config: config.Config;
	
	    static createFrom(source: any = {}) {
	        return new ConfigInfo(source);
	    }
	
	    constructor(source: any = {}) {
	        if ('string' === typeof source) source = JSON.parse(source);
	        this.working_dir = source["working_dir"];
	        this.selected_profile_index = source["selected_profile_index"];
	        this.profiles = this.convertValues(source["profiles"], Profile);
	        this.env_config = this.convertValues(source["env_config"], config.Config);
	    }
	
		convertValues(a: any, classs: any, asMap: boolean = false): any {
		    if (!a) {
		        return a;
		    }
		    if (a.slice && a.map) {
		        return (a as any[]).map(elem => this.convertValues(elem, classs));
		    } else if ("object" === typeof a) {
		        if (asMap) {
		            for (const key of Object.keys(a)) {
		                a[key] = new classs(a[key]);
		            }
		            return a;
		        }
		        return new classs(a);
		    }
		    return a;
		}
	}

}

