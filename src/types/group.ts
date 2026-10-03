/** A folder as `get_groups` returns it — mirrors mypass-core's GroupInfo. */
export interface Group {
  uuid: string;
  name: string;
  icon: string | null;
  children: Group[];
  /** Entries of this folder and of all its subfolders. */
  entryCount: number;
  isExpanded: boolean;
  created: string | null;
  modified: string | null;
}
