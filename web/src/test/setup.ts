import { cleanup } from '@testing-library/react';
import { afterEach } from 'vitest';

// 每个用例结束后卸载渲染的组件，避免互相影响
afterEach(cleanup);
