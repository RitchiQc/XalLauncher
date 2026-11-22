import { confirmAction } from "@/lib/dialog";

export function confirmCodeExecution(content: string): Promise<boolean> {
  return confirmAction(
    `${content} contient du code Java qui sera exécuté avec Minecraft. ` +
    "Il pourra accéder aux fichiers du jeu et au jeton de session Minecraft. " +
    "N’installez que du contenu dont vous faites confiance. Continuer ?",
    "Contenu exécutable",
    "warning",
  );
}

export function confirmHorizonContent(content: string): Promise<boolean> {
  return confirmAction(
    `${content} sera chargé par Horizon. Les mods de contenu .hmod sont sandboxés, ` +
    "mais ils restent du contenu exécutable fourni par un tiers. " +
    "N’installez que du contenu dont vous faites confiance. Continuer ?",
    "Contenu Horizon",
    "warning",
  );
}

export function confirmHorizonNative(content: string): Promise<boolean> {
  return confirmAction(
    `${content} est un mod natif Horizon. Son paquet source .hmod sera installé pour un prochain build natif : ` +
    "il sera compilé et exécuté avec les droits de votre compte, sans sandbox. " +
    "N’installez que du contenu dont vous faites confiance. Continuer ?",
    "Mod natif Horizon",
    "warning",
  );
}

export function confirmHorizonModpack(content: string): Promise<boolean> {
  return confirmAction(
    `${content} installe des mods de contenu et des mods natifs Horizon. ` +
    "Les mods natifs nécessitent un build Horizon avant de pouvoir se charger et s’exécuteront " +
    "avec les droits de votre compte, sans sandbox. Continuer ?",
    "Modpack Horizon",
    "warning",
  );
}
