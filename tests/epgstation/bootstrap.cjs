'use strict';
// Only prepares private data and starts the real provider HTTP service. No
// provider bindings, HTTP handlers, authentication or DB models are replaced.
// The recorder, EPG updater and encoder detection are deliberately not started:
// this suite tests previously recorded files and needs no tuner/GPU/audio.
require('reflect-metadata');
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');

const mode = process.env.CONTRACT_AUTH;
assert.ok(mode === 'anonymous' || mode === 'password', 'explicit auth mode required');
const { FEATURE_FLAG_KEYS } = require('./dist/model/IConfigFile');
fs.mkdirSync('data', { recursive: true });
fs.writeFileSync('config/config.yml', JSON.stringify({
    port: 8888,
    dbtype: 'sqlite',
    subDirectory: mode === 'password' ? '/protected' : '/',
    auth: { enabled: mode === 'password', allowAnonymous: false },
    recorded: [{ name: 'recorded', path: '/app/recorded' }],
    // No background integrations are part of this API contract suite.
    featureFlags: Object.fromEntries(FEATURE_FLAG_KEYS.map(key => [key, false])),
}));

const container = require('./dist/model/ModelContainer').default;
require('./dist/model/ModelContainerSetter').set(container);
const logger = container.get('ILoggerModel');
logger.initialize();
// The provider's access log includes token query strings. Do not record them.
logger.getLogger().access.level = 'off';

async function main() {
    const connection = await container.get('IDBOperator').getConnection();
    const Channel = require('./dist/db/entities/Channel').default;
    const Recorded = require('./dist/db/entities/Recorded').default;
    const VideoFile = require('./dist/db/entities/VideoFile').default;
    const Reserve = require('./dist/db/entities/Reserve').default;
    const Program = require('./dist/db/entities/Program').default;
    assert.equal(await connection.getRepository(Recorded).count(), 0, 'database must be fresh');
    await connection.getRepository(Channel).save({
        id: 400101, networkId: 4, serviceId: 101,
        name: 'Test TV', halfWidthName: 'Test TV', channelTypeId: 1,
        channelType: 'BS', channel: 'BS15_0', type: 1, hasLogoData: false,
    });
    // Values are synthetic; schema, serialization and file delivery are upstream.
    // More than one client page exercises actual search, ordering and pagination.
    for (let id = 1; id <= 52; id++) {
        const startAt = 1700000000000 - id * 60000;
        const name = id <= 12 ? `EPGStation recording ${id}` : `Pagination ${id}`;
        await connection.getRepository(Recorded).save({
            id, channelId: 400101, channelName: 'Test TV', halfWidthChannelName: 'Test TV',
            name, halfWidthName: name, startAt, endAt: startAt + 60000, duration: 60000,
            description: 'Recorded programme with a description for browsing.',
            halfWidthDescription: 'Recorded programme with a description for browsing.',
            isRecording: false, isProtected: false,
        });
        const files = id === 2
            ? [[126, 'recording-seek.ts', 'ts'], [124, 'media-h264.mp4', 'encoded']]
            : id === 3 ? [[125, 'media-hevc.mkv', 'encoded']]
            : [[id === 1 ? 123 : 200 + id, 'recording-seek.ts', 'ts']];
        for (const [videoId, filename, type] of files) {
            await connection.getRepository(VideoFile).save({
                id: videoId, recordedId: id, parentDirectoryName: 'recorded',
                filePath: filename, type, name: type === 'ts' ? 'TS' : filename,
                size: fs.statSync(path.join('recorded', filename)).size,
                // Explicit fixture metadata avoids invoking ffprobe at startup.
                duration: 12, startTime: 0, startAt, analyzedAt: 1700000000000,
            });
        }
    }
    // Reservation reads exercise real DB/API serialization without starting a
    // tuner or the operator IPC that executes reservation writes.
    for (let id = 1; id <= 4; id++) {
        const programId = 40010100000 + id;
        const startAt = 1700000000000 + id * 600000;
        await connection.getRepository(Program).save({
            id: programId, updateTime: 1700000000000, channelId: 400101,
            eventId: id, serviceId: 101, networkId: 4, channel: 'BS15_0', channelType: 'BS',
            startAt, endAt: startAt + 600000, duration: 600000,
            startHour: 0, week: 1, isFree: true,
            name: `Reservation ${id}`, halfWidthName: `Reservation ${id}`, shortName: `Reservation ${id}`,
        });
        await connection.getRepository(Reserve).save({
            id, programId, updateTime: 1700000000000, channelId: 400101,
            channel: 'BS15_0', channelType: 'BS', startAt, endAt: startAt + 600000,
            name: `Reservation ${id}`, halfWidthName: `Reservation ${id}`,
            isConflict: id === 2, isSkip: id === 3,
        });
        if (id === 4) {
            await connection.getRepository(Recorded).save({
                id: 900, programId, channelId: 400101, channelName: 'Test TV', halfWidthChannelName: 'Test TV',
                name: 'Recording in progress', halfWidthName: 'Recording in progress',
                startAt, endAt: startAt + 600000, duration: 600000,
                isRecording: true, isProtected: false,
            });
        }
    }
    container.get('IServiceServer').start();
}

if (typeof process.send === 'undefined') {
    // The provider service expects a child-process IPC channel. This suite does
    // not execute operator operations; any accidental use must fail explicitly.
    const child = require('node:child_process').fork(__filename, [], {
        stdio: ['ignore', 'inherit', 'inherit', 'ipc'],
    });
    child.on('message', () => {
        console.error('Unexpected operator IPC request in the recorded-file API suite');
        child.kill();
        process.exitCode = 1;
    });
    child.on('error', error => {
        console.error('Failed to start EPGStation API child:', error);
        process.exit(1);
    });
    child.on('exit', code => process.exit(code === 0 ? 0 : 1));
} else {
    main().catch(error => {
        console.error('EPGStation contract provider startup failed:', error);
        process.exit(1);
    });
}
