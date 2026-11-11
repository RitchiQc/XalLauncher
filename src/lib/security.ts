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
