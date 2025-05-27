import { html, Html } from "@elysiajs/html";
import Elysia, { file, status } from "elysia";
import type { ElysiaWS } from "elysia/ws";
import { Type, type Static } from "@sinclair/typebox";
import scr from "./script.txt" with { type: "text" };
import fs from "fs";
import { Value } from "@sinclair/typebox/value";
import staticPlugin from "@elysiajs/static";

const $throw = (err: Error): never => {
  throw err;
};

type Args = {
  config: string,
  port?: number,
  password?: string,
  control?: true
};

const Config = Type.Object({
  name: Type.String(),
  password: Type.Optional(Type.String({ minLength: 5, maxLength: 30 })),
  control: Type.Optional(Type.Boolean()),
  slides: Type.Array(Type.TemplateLiteral('${string}.html'), { minItems: 1 }),
  slideRatio: Type.Object({width: Type.Number({ minimum: 1 }), height: Type.Number({ minimum: 1 })}),
  background: Type.Optional(Type.String())
});

type Config = Static<typeof Config>;

const parseArgs = (argv: string[]): Args => {
  const args: Args = {
    config: "skate.json"
  };
  let i = 0;
  while (i < argv.length) {
    const arg = argv[i]!;
    switch (arg) {
      case "--help": case "-h": {
        console.log(`Usage: ${process.argv[0]} [CONFIG_FILE] [options]
Options:
  --password | -P <pass>  Password to control the presentation flow on other devices.
  --port | -p <port>      Port to listen on. Default: 3000.
  --control | -c          Defines if clients are allowed to move between slides.

All options overwrite their equivalent on the config file if passed as args`);
        process.exit(0);
      }
      case "--password": case "-P": {
        args.password = argv[++i] ?? $throw(new Error("Missing password"));
      } break;
      case "--control": {
        args.control = true;
      } break;
      case "--port": case "-p": {
        args.port = parseInt(argv[++i] ?? $throw(new Error("Missing port"))) || $throw(new Error("Invalid port"));
      } break;
      default: {
        args.config = arg;
      }
    }
    i++;
  }
  return args;
}

const args: Args = parseArgs(process.argv.slice(2));

const config = JSON.parse(fs.readFileSync(args.config, { encoding: "utf-8" }));

Value.Assert(Config, config);

const script = args.control
  ? `${scr.replace("LENGTH", (config.slides.length - 1).toString())}()`
  : scr.replace("LENGTH", "0");


const clients = new Set<ElysiaWS>();

new Elysia()
  .use(html())
  .use(staticPlugin({ prefix: "/", assets: "./" }))
  .get("/", () => <html>
    <head>
      <title>{config.name}</title>
      <meta charset="utf-8" />
      <meta name="viewport" content="width=device-width, initial-scale=1" />
      <meta name="description" content={config.name} />
      <script src="/script.js" type="module" defer></script>
      <style>
        body {`{
margin: 0;
padding: 0;
overflow: hidden;
background: ${config.background ?? "#111122"};
background-size: cover;
}`}

        iframe {`{
position: absolute;
top: 50%;
left: 50%;
transform: translate(-50%, -50%);
aspect-ratio: ${config.slideRatio.width} / ${config.slideRatio.height};
border: none;
outline: none;
height: 100%;
}`}
      </style>
    </head>
    <body>
      <iframe src="/0" />
    </body>
  </html>)
  .get("/script.js", ({ set }) => {
    set.headers["Content-Type"] = "text/javascript";
    return script;
  })
  .get("/:slide", ({ params: { slide } }) => file(config.slides[slide]!),
    {
      params: Type.Object({
        slide: Type.Number({ minimum: 0, maximum: config.slides.length - 1 })
      })
    })
  .get("/goto/:slide", ({ params: { slide }, query: { pass } }) => {
    if (pass === config.password) {
      for (const client of clients) {
        client.send(JSON.stringify(slide));
      }
      return <h1>Ok!</h1>;
    }
    return status(401);
  }, {
      params: Type.Object({
        slide: Type.Number({ minimum: 0, maximum: config.slides.length - 1 })
      }),
      query: Type.Object({
        pass: Type.Optional(Type.String())
      })
    })
  .ws("/", {
    open(ws) {
      clients.add(ws);
    },
    close(ws) {
      clients.delete(ws);
    }
  }).listen(args.port ?? 3000, () => console.log(`Listening on http://localhost:${args.port ?? 3000}`));
