# Skate

*Slide in style with **HTML** presentations served with **Elysia**.*

Building:
```bash
git clone https://github.com/Destinaria/skate.git
cd skate
bun compile
```

Skate uses a JSON configuration file named `skate.json` for configuring general info about the slideshow. The available options are:
- `name`: Used for the title of HTML webpage (required);
- `password`: Defines a password for changing the current slide remotely (optional);
- `controls`: Enables/disables the slide controls on clients (optional);
- `slides`: Defines the HTML files for each slide in order (required);
- `slideRatio`: Defines the width/height ratio for the slides (required);
- `background`: Defines the background CSS property for the page *containing* the slides, not the slides themselves (optional);

NOTE: having controls disabled (default behavior) and not having a password means you won't be able to progress the presentation at all.

Skate is a silly small personal project. Any contributions or issues are welcome.
