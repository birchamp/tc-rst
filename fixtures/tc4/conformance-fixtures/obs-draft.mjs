// The drafted content of sample-burrito-obs/ingredients/content/01.md (issue #147): the
// title, frames 1 and 2, the reference line. Shared by generate-obs.mjs (which writes it
// through the §10 writers) and validate.mjs (which folds the same content as `v: 2`
// events and expects the projection to equal the file byte for byte).
// Spanish sample text, CC BY-SA 4.0, Equipo Ejemplo (../LICENSE-CONTENT.md). Frame 1 keeps a
// single newline inside its paragraph (R-10.3.2: single newlines survive).
export const DRAFT = {
  title: 'La Creación',
  frames: {
    1: 'Así fue como Dios hizo todo en el principio. Creó el universo y todo lo que hay en él en seis días.\nDespués de que Dios creó la tierra, estaba oscura y vacía, porque él aún no había formado nada en ella. Pero el Espíritu de Dios estaba allí sobre el agua.',
    2: 'Entonces Dios dijo: «Que haya luz». Y hubo luz. Dios vio que la luz era buena y la llamó «día». La separó de la oscuridad, a la que llamó «noche». Dios creó la luz en el primer día de la creación.',
  },
  ref: 'Una historia bíblica de: Génesis 1-2',
};
