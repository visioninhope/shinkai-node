import {execMode, execModeConfig, validate} from './exec-mode';
import fs from 'fs/promises';
// Http Mode
import express from 'express';
import bodyParser from 'body-parser';
import {IncomingHttpHeaders} from 'http';

export function httpMode(port: string | number) {
  const app = express();
  app.use(bodyParser.json({limit: '50mb'}));

  app.post(
    '/validate_headers',
    async (
      req: express.Request<{}, {}, {source: string}>,
      res: express.Response
    ) => {
      if (!req.body.source)
        return res.status(400).json({error: 'Missing source'});

      const response = await runWithSource(
        req.body.source,
        async path => await validate(path, filterHeaders(req.headers))
      );

      return res.json(JSON.parse(response));
    }
  );

  app.post(
    '/toolkit_json',
    async (
      req: express.Request<{}, {}, {source: string}>,
      res: express.Response
    ) => {
      if (!req.body.source)
        return res.status(400).json({error: 'Missing source'});

      const response = await runWithSource(
        req.body.source,
        async path => await execModeConfig(path)
      );

      return res.json(JSON.parse(response));
    }
  );

  app.post(
    '/execute_tool',
    async (
      req: express.Request<
        {},
        {},
        {source: string; tool: string; input: string}
      >,
      res: express.Response
    ) => {
      if (!req.body) return res.status(400).json({error: 'Missing body'});
      if (!req.body.source)
        return res.status(400).json({error: 'Missing source'});
      if (!req.body.tool) return res.status(400).json({error: 'Missing tool'});

      const response = await runWithSource(
        req.body.source,
        async path =>
          await execMode(
            path,
            req.body.tool,
            JSON.stringify(req.body.input || {}),
            filterHeaders(req.headers)
          )
      );

      return res.json(JSON.parse(response));
    }
  );

  app.all(
    '/health_check',
    async (req: express.Request, res: express.Response) => {
      return res.json({status: true});
    }
  );

  app.listen(parseInt(String(port), 10), () => {
    console.log(`Listening at http://localhost:${port}`);
  });
}

const filterHeaders = (rawHeaders: IncomingHttpHeaders): string => {
  const headers: Record<string, string | string[] | undefined> = {};
  Object.keys(rawHeaders || {}).forEach(h => {
    if (h.match(/^x-shinkai-.*/)) {
      headers[h] = rawHeaders[h];
    }
  });
  return JSON.stringify(headers);
};

const runWithSource = async <T>(
  source: string,
  callback: (path: string) => Promise<T>
): Promise<T> => {
  const path = `./tmp_${new Date().getTime()}_${String(Math.random()).replace(
    /0./,
    ''
  )}.js`;

  await fs.writeFile(path, source, 'utf8');

  let data: T;

  try {
    data = await callback(path);
  } finally {
    // Ensure the temporary file is deleted
    try {
      await fs.unlink(path);
    } catch (err) {
      console.error(`Failed to delete temporary file: ${path}. Error:`, err);
    }
  }

  return data;
};
