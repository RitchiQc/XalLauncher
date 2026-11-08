export function confirmCodeExecution(content: string): boolean {
  return window.confirm(
    `${content} contient du code Java qui sera exécuté avec Minecraft. ` +
    "Il pourra accéder aux fichiers du jeu et au jeton de session Minecraft. " +
    "N’installez que du contenu dont vous faites confiance. Continuer ?",
  );
}
