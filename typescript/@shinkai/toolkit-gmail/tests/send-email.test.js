const {
  ShinkaiTookitLib,
  GmailSendEmail,
} = require('./../dist/packaged-shinkai-toolkit');

describe('CreateQuick Event Test', () => {
  test('check object', async () => {
    // await ShinkaiTookitLib.waitForLib();
    const config = await ShinkaiTookitLib.emitConfig();

    expect(JSON.parse(config).tools[0].name).toEqual(
      new GmailSendEmail().constructor.name
    );
  });
});
