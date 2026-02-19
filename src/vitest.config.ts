import { resolve } from 'path';

const root = __dirname;

export default {
  resolve: {
    alias: {
      '@commands': resolve(root, 'commands'),
      '@daemons': resolve(root, 'daemons'),
      '@system': resolve(root, 'system'),
      '@widgets': resolve(root, 'widgets'),
      '@shared': resolve(root, 'shared'),
      '@types': resolve(root, 'types'),
      '@browser': resolve(root, 'browser'),
      '@server': resolve(root, 'server'),
      '@generator': resolve(root, 'generator'),
      '@scripts': resolve(root, 'scripts'),
      '@utils': resolve(root, 'utils'),
      '@commands-utilities': resolve(root, 'commands/utilities'),
      '@commands-workspace': resolve(root, 'commands/workspace'),
      '@commands-interface': resolve(root, 'commands/interface'),
      '@commands-collaboration': resolve(root, 'commands/collaboration'),
      '@commands-development': resolve(root, 'commands/development'),
    },
  },
  test: {
    root,
  },
};
