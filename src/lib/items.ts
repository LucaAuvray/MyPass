/** Types d'éléments MyPass (identités / cartes / documents), stockés en
 *  entrées KDBX discriminées par le champ personnalisé MyPass_Type. */

export type ItemKind = "login" | "identity" | "card" | "document" | "ssh_key";

export const MYPASS_TYPE_KEY = "MyPass_Type";

export const IDENTITY_FIELDS = [
  "ID_FirstName",
  "ID_LastName",
  "ID_BirthDate",
  "ID_Email",
  "ID_Phone",
  "ID_Address",
  "ID_City",
  "ID_PostalCode",
  "ID_Country",
  "ID_Company",
] as const;

export const CARD_FIELDS = [
  "CC_Holder",
  "CC_Number",
  "CC_ExpMonth",
  "CC_ExpYear",
  "CC_CVC",
] as const;

export const DOCUMENT_FIELDS = [
  "DOC_Kind",
  "DOC_Number",
  "DOC_IssueDate",
  "DOC_ExpiryDate",
  "DOC_Country",
] as const;

/** Champs affichés d'une clé SSH ; SSH_PrivateKey est géré à part (secret,
 *  jamais éditable en formulaire — importé ou généré uniquement). */
export const SSH_FIELDS = [
  "SSH_PublicKey",
  "SSH_Fingerprint",
  "SSH_Algorithm",
  "SSH_Comment",
] as const;

/** Masqués par défaut dans l'UI ; protégés côté KDBX (voir entries.rs). */
export const SECRET_FIELDS: string[] = ["CC_Number", "CC_CVC", "DOC_Number", "SSH_PrivateKey"];

export const DOC_KINDS = [
  "passport",
  "id_card",
  "driver_license",
  "social_security",
  "other",
] as const;

export function itemKind(entry: { customFields?: Record<string, string> }): ItemKind {
  const t = entry.customFields?.[MYPASS_TYPE_KEY];
  return t === "identity" || t === "card" || t === "document" || t === "ssh_key" ? t : "login";
}

export function cardBrand(number: string): "visa" | "mastercard" | "amex" | "other" {
  const n = number.replace(/\s/g, "");
  if (n.startsWith("4")) return "visa";
  if (/^(5[1-5]|2[2-7])/.test(n)) return "mastercard";
  if (/^3[47]/.test(n)) return "amex";
  return "other";
}

export function maskCardNumber(number: string): string {
  const n = number.replace(/\s/g, "");
  return n.length >= 4 ? `•••• ${n.slice(-4)}` : "••••";
}
