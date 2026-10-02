export interface Group {
  uuid: string;
  name: string;
  parent?: string;
  icon: number;
  children: Group[];
  entries: string[];
  isExpanded: boolean;
  created: string;
  modified: string;
  notes?: string;
  browserSettings?: GroupBrowserSettings;
}

export interface GroupBrowserSettings {
  excludeFromBrowser: boolean;
  allowAutoType: boolean;
}
