#!/usr/bin/env node

const http = require('http');

// Test the version endpoint
const options = {
  hostname: 'localhost',
  port: 5555,
  path: '/version',
  method: 'GET'
};

const req = http.request(options, (res) => {
  console.log(`Status: ${res.statusCode}`);
  
  let data = '';
  res.on('data', (chunk) => {
    data += chunk;
  });
  
  res.on('end', () => {
    console.log('Response:', data);
    try {
      const json = JSON.parse(data);
      console.log('Parsed:', json);
    } catch (e) {
      console.log('Not JSON:', data);
    }
  });
});

req.on('error', (e) => {
  console.error(`Request error: ${e.message}`);
});

req.end();