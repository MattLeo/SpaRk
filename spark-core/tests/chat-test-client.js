const WebSocket = require('ws');
const readline = require('readline');
const net = require('net');

class AuthClient {
  constructor(host = '127.0.0.1', port = 8080) {
    this.host = host;
    this.port = port;
  }

  async sendRequest(request) {
    return new Promise((resolve, reject) => {
      const client = new net.Socket();
      let data = '';
      let resolved = false;

      const timeout = setTimeout(() => {
        if (!resolved) {
          client.destroy();
          reject(new Error('Request timeout'));
        }
      }, 10000);

      client.connect(this.port, this.host, () => {
        client.write(JSON.stringify(request));
      });

      client.on('data', (chunk) => {
        data += chunk.toString();
        
        try {
          const response = JSON.parse(data);
          if (!resolved) {
            resolved = true;
            clearTimeout(timeout);
            client.destroy();
            
            if (response.status === 'Success') {
              resolve(response.data);
            } else {
              reject(new Error(response.message || 'Request failed'));
            }
          }
        } catch (error) {
        }
      });

      client.on('error', (error) => {
        if (!resolved) {
          resolved = true;
          clearTimeout(timeout);
          reject(error);
        }
      });

      client.on('close', () => {
        if (!resolved) {
          resolved = true;
          clearTimeout(timeout);
          reject(new Error('Connection closed without response'));
        }
      });
    });
  }

  async register(username, email, password) {
    return this.sendRequest({
      type: 'Register',
      username,
      email,
      password
    });
  }

  async login(username, password) {
    return this.sendRequest({
      type: 'Login',
      username,
      password
    });
  }
}

class ChatTestClient {
  constructor(wsUrl, username, password) {
    this.wsUrl = wsUrl;
    this.username = username;
    this.password = password;
    this.email = `${username}@test.com`;
    this.ws = null;
    this.connected = false;
    this.authenticated = false;
    this.token = null;
    this.userId = null;
    this.currentRoom = null;
    this.authClient = new AuthClient();
    this.lastRoomCreated = null;
    this.availableRooms = [];
  }

  async authenticate() {
    try {
      const loginData = await this.authClient.login(this.username, this.password);
      this.token = loginData.token;
      console.log(`[${this.username}] Logged in successfully`);
      return this.token;
    } catch (loginError) {
      try {
        console.log(`[${this.username}] Login failed, attempting registration...`);
        await this.authClient.register(this.username, this.email, this.password);
        const loginData = await this.authClient.login(this.username, this.password);
        this.token = loginData.token;
        console.log(`[${this.username}] Registered and logged in successfully`);
        return this.token;
      } catch (regError) {
        if (regError.message.includes('User already exists')) {
          console.log(`[${this.username}] User exists but password incorrect, trying default password...`);
          try {
            const retryLogin = await this.authClient.login(this.username, 'password123');
            this.token = retryLogin.token;
            console.log(`[${this.username}] Logged in with default password`);
            return this.token;
          } catch (finalError) {
            throw new Error(`Authentication failed: User exists but cannot login. Try deleting the database or using a different username.`);
          }
        }
        throw new Error(`Authentication failed: ${regError.message}`);
      }
    }
  }

  async connect() {
    await this.authenticate();

    return new Promise((resolve, reject) => {
      this.ws = new WebSocket(this.wsUrl);

      this.ws.on('open', () => {
        this.connected = true;
        console.log(`[${this.username}] Connected to WebSocket server`);
        
        this.send({
          type: 'Authenticate',
          token: this.token
        });
        
        setTimeout(() => resolve(), 500);
      });

      this.ws.on('message', (data) => {
        try {
          const message = JSON.parse(data.toString());
          this.handleMessage(message);
        } catch (error) {
          console.error(`[${this.username}] Error parsing message:`, error);
        }
      });

      this.ws.on('error', (error) => {
        console.error(`[${this.username}] WebSocket error:`, error.message);
        reject(error);
      });

      this.ws.on('close', () => {
        this.connected = false;
        this.authenticated = false;
        console.log(`[${this.username}] Disconnected from server`);
      });
    });
  }

  handleMessage(message) {
    switch (message.type) {
      case 'Authenticated':
        this.authenticated = true;
        this.userId = message.user_id;
        console.log(`[${this.username}] Authenticated as ${message.username} (${message.user_id})`);
        break;
      
      case 'RoomCreated':
        this.lastRoomCreated = message.room_id;
        console.log(`[${this.username}] Room created: ${message.room_name} (${message.room_id})`);
        break;
      
      case 'RoomList':
        this.availableRooms = message.rooms;
        console.log(`[${this.username}] Available rooms: ${message.rooms.length}`);
        message.rooms.forEach(room => {
          console.log(`  - ${room.name} (${room.id}): ${room.desc}`);
        });
        break;
      
      case 'RoomJoined':
        this.currentRoom = message.room_id;
        console.log(`[${this.username}] Joined room: ${message.room_name}`);
        break;
      
      case 'RoomLeft':
        console.log(`[${this.username}] Left room: ${message.room_id}`);
        this.currentRoom = null;
        break;
      
      case 'UserJoined':
        console.log(`[${this.username}] ${message.username} joined ${message.room_id}`);
        break;
      
      case 'UserLeft':
        console.log(`[${this.username}] ${message.username} left ${message.room_id}`);
        break;
      
      case 'NewMessage':
        const msg = message.message;
        console.log(`[${this.username}] [${message.room_id}] ${msg.sender_username}: ${msg.content}`);
        break;
      
      case 'MessageSent':
        console.log(`[${this.username}] Message sent (${message.message_id})`);
        break;
      
      case 'RoomHistory':
        console.log(`[${this.username}] Room history (${message.messages.length} messages)`);
        message.messages.forEach(msg => {
          console.log(`  ${msg.sender_username}: ${msg.content}`);
        });
        break;
      
      case 'Error':
        console.error(`[${this.username}] Error: ${message.message}`);
        break;
      
      default:
        console.log(`[${this.username}] Unknown message type:`, message);
    }
  }

  send(data) {
    if (this.connected && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(data));
    } else {
      console.error(`[${this.username}] Cannot send message: not connected`);
    }
  }

  getAllRooms() {
    this.send({ type: 'GetAllRooms' });
  }

  createRoom(name, desc) {
    this.send({
      type: 'CreateRoom',
      name,
      desc
    });
  }

  joinRoom(roomId) {
    this.send({
      type: 'JoinRoom',
      room_id: roomId
    });
  }

  leaveRoom(roomId) {
    this.send({
      type: 'LeaveRoom',
      room_id: roomId
    });
  }

  sendMessage(roomId, content) {
    this.send({
      type: 'SendMessage',
      room_id: roomId,
      content
    });
  }

  getRoomHistory(roomId, limit = 50, offset = 0) {
    this.send({
      type: 'GetRoomHistory',
      room_id: roomId,
      limit,
      offset
    });
  }

  disconnect() {
    if (this.ws) {
      this.ws.close();
    }
  }
}

class ChatTester {
  constructor(wsServerUrl) {
    this.wsServerUrl = wsServerUrl;
    this.clients = [];
    this.testRoomId = null;
  }

  async runBasicTest() {
    console.log('\n=== Running Basic Connection Test ===\n');
    
    const client = new ChatTestClient(this.wsServerUrl, 'TestUser1', 'password123');
    this.clients.push(client);
    
    await client.connect();
    await this.sleep(1000);
    
    client.createRoom('Test Room', 'A room for testing');
    await this.sleep(1000);
    
    client.disconnect();
    await this.sleep(1000);
  }

  async runMultiUserTest() {
    console.log('\n=== Running Multi-User Test ===\n');
    
    const usernames = ['Alice', 'Bob', 'Charlie'];
    const clients = [];
    
    console.log('--- Connecting users ---\n');
    for (const username of usernames) {
      const client = new ChatTestClient(this.wsServerUrl, username, 'password123');
      clients.push(client);
      this.clients.push(client);
      
      await client.connect();
      await this.sleep(500);
    }
    
    await this.sleep(1000);
    
    console.log('\n--- Creating room ---\n');
    clients[0].createRoom('Chat Room', 'Multi-user test room');
    await this.sleep(1000);
    
    clients[0].getAllRooms();
    await this.sleep(1000);
    
    console.log('\n--- Manual step: Note the room ID from above and update the code ---\n');
    
    const roomId = 'REPLACE_WITH_ACTUAL_ROOM_ID';
    
    console.log('\n--- Users joining room ---\n');
    for (const client of clients) {
      await this.sleep(500);
    }
    
    await this.sleep(1000);
    
    console.log('\n--- Users chatting ---\n');
    
    for (const client of clients) {
      client.disconnect();
      await this.sleep(300);
    }
    
    await this.sleep(1000);
  }

  async runStressTest(numUsers = 5, messagesPerUser = 3) {
    console.log(`\n=== Running Stress Test (${numUsers} users, ${messagesPerUser} messages each) ===\n`);
    
    const clients = [];
    const timestamp = Date.now();
    
    console.log('Connecting users sequentially to avoid overload...');
    for (let i = 0; i < numUsers; i++) {
      const username = `StressUser${timestamp}_${i + 1}`;
      const client = new ChatTestClient(this.wsServerUrl, username, 'password123');
      clients.push(client);
      this.clients.push(client);
      
      try {
        await client.connect();
        await this.sleep(100);
      } catch (error) {
        console.error(`Failed to connect ${username}:`, error.message);
      }
    }
    
    const connectedClients = clients.filter(c => c.connected);
    console.log(`\nSuccessfully connected ${connectedClients.length}/${numUsers} users\n`);
    
    if (connectedClients.length === 0) {
      console.log('No clients connected, aborting test');
      return;
    }
    
    await this.sleep(1000);
    
    console.log('Creating test room...');
    connectedClients[0].createRoom(`Stress Test ${timestamp}`, 'Room for stress testing');
    await this.sleep(1000);
    
    const roomId = connectedClients[0].lastRoomCreated;
    if (!roomId) {
      console.log('Failed to get room ID, aborting test');
      return;
    }
    
    console.log(`\nRoom created with ID: ${roomId}`);
    console.log('All users joining room...\n');
    
    for (const client of connectedClients) {
      client.joinRoom(roomId);
      await this.sleep(300);
    }
    
    await this.sleep(2000);
    
    console.log('\nUsers sending messages...\n');
    for (let msg = 0; msg < messagesPerUser; msg++) {
      for (const client of connectedClients) {
        client.sendMessage(roomId, `Message ${msg + 1} from ${client.username}`);
        await this.sleep(50);
      }
    }
    
    await this.sleep(2000);
    
    console.log('\nGetting room history...\n');
    connectedClients[0].getRoomHistory(roomId);
    await this.sleep(1000);
    
    console.log('\nDisconnecting users...');
    for (const client of clients) {
      client.disconnect();
    }
    
    await this.sleep(1000);
    
    console.log(`\nStress test complete! ${connectedClients.length} users sent ${messagesPerUser} messages each (${connectedClients.length * messagesPerUser} total messages)`);
  }

  async runInteractiveMode() {
    console.log('\n=== Interactive Mode ===\n');
    console.log('Commands:');
    console.log('  /connect <username> - Connect a new user');
    console.log('  /rooms <username> - Get list of all rooms');
    console.log('  /create <username> <roomname> <description> - Create a room');
    console.log('  /join <username> <roomid> - Join a room');
    console.log('  /leave <username> <roomid> - Leave a room');
    console.log('  /msg <username> <roomid> <message> - Send a message');
    console.log('  /history <username> <roomid> - Get room history');
    console.log('  /disconnect <username> - Disconnect a user');
    console.log('  /list - List all connected users');
    console.log('  /quit - Exit interactive mode\n');
    
    const rl = readline.createInterface({
      input: process.stdin,
      output: process.stdout
    });
    
    const clientMap = new Map();
    
    const processCommand = async (line) => {
      const parts = line.trim().split(' ');
      const command = parts[0];
      
      try {
        switch (command) {
          case '/connect':
            if (parts.length < 2) {
              console.log('Usage: /connect <username>');
              break;
            }
            const username = parts[1];
            if (clientMap.has(username)) {
              console.log(`User ${username} already connected`);
              break;
            }
            const client = new ChatTestClient(this.wsServerUrl, username, 'password123');
            this.clients.push(client);
            await client.connect();
            clientMap.set(username, client);
            break;
          
          case '/rooms':
            if (parts.length < 2) {
              console.log('Usage: /rooms <username>');
              break;
            }
            if (!clientMap.has(parts[1])) {
              console.log(`User ${parts[1]} not found`);
              break;
            }
            clientMap.get(parts[1]).getAllRooms();
            break;
          
          case '/create':
            if (parts.length < 4) {
              console.log('Usage: /create <username> <roomname> <description>');
              break;
            }
            if (!clientMap.has(parts[1])) {
              console.log(`User ${parts[1]} not found`);
              break;
            }
            const roomName = parts[2];
            const roomDesc = parts.slice(3).join(' ');
            clientMap.get(parts[1]).createRoom(roomName, roomDesc);
            break;
          
          case '/join':
            if (parts.length < 3) {
              console.log('Usage: /join <username> <roomid>');
              break;
            }
            if (!clientMap.has(parts[1])) {
              console.log(`User ${parts[1]} not found`);
              break;
            }
            clientMap.get(parts[1]).joinRoom(parts[2]);
            break;
          
          case '/leave':
            if (parts.length < 3) {
              console.log('Usage: /leave <username> <roomid>');
              break;
            }
            if (!clientMap.has(parts[1])) {
              console.log(`User ${parts[1]} not found`);
              break;
            }
            clientMap.get(parts[1]).leaveRoom(parts[2]);
            break;
          
          case '/msg':
            if (parts.length < 4) {
              console.log('Usage: /msg <username> <roomid> <message>');
              break;
            }
            if (!clientMap.has(parts[1])) {
              console.log(`User ${parts[1]} not found`);
              break;
            }
            const message = parts.slice(3).join(' ');
            clientMap.get(parts[1]).sendMessage(parts[2], message);
            break;
          
          case '/history':
            if (parts.length < 3) {
              console.log('Usage: /history <username> <roomid>');
              break;
            }
            if (!clientMap.has(parts[1])) {
              console.log(`User ${parts[1]} not found`);
              break;
            }
            clientMap.get(parts[1]).getRoomHistory(parts[2]);
            break;
          
          case '/disconnect':
            if (parts.length < 2) {
              console.log('Usage: /disconnect <username>');
              break;
            }
            if (!clientMap.has(parts[1])) {
              console.log(`User ${parts[1]} not found`);
              break;
            }
            clientMap.get(parts[1]).disconnect();
            clientMap.delete(parts[1]);
            break;
          
          case '/list':
            console.log('Connected users:', Array.from(clientMap.keys()).join(', '));
            break;
          
          case '/quit':
            console.log('Exiting interactive mode...');
            for (const client of clientMap.values()) {
              client.disconnect();
            }
            rl.close();
            return;
          
          default:
            console.log('Unknown command. Available commands:');
            console.log('/connect, /rooms, /create, /join, /leave, /msg, /history, /disconnect, /list, /quit');
        }
      } catch (error) {
        console.error('Error executing command:', error.message);
      }
      
      rl.prompt();
    };
    
    rl.on('line', processCommand);
    rl.on('close', () => {
      console.log('Interactive mode ended');
    });
    
    rl.setPrompt('> ');
    rl.prompt();
  }

  sleep(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  cleanup() {
    console.log('\nCleaning up connections...');
    for (const client of this.clients) {
      client.disconnect();
    }
  }
}

async function main() {
  const wsServerUrl = process.env.WS_SERVER_URL || 'ws://localhost:8081';
  
  console.log(`Chat Testing Application`);
  console.log(`WebSocket Server: ${wsServerUrl}`);
  console.log(`Auth Server: 127.0.0.1:8080\n`);
  
  const tester = new ChatTester(wsServerUrl);
  
  const args = process.argv.slice(2);
  const mode = args[0] || 'basic';
  
  try {
    switch (mode) {
      case 'basic':
        await tester.runBasicTest();
        break;
      
      case 'multi':
        await tester.runMultiUserTest();
        break;
      
      case 'stress':
        const numUsers = parseInt(args[1]) || 5;
        const messagesPerUser = parseInt(args[2]) || 3;
        await tester.runStressTest(numUsers, messagesPerUser);
        break;
      
      case 'interactive':
        await tester.runInteractiveMode();
        return;
      
      default:
        console.log('Usage: node chat-test-client.js [mode] [options]');
        console.log('Modes:');
        console.log('  basic - Basic connection and authentication test');
        console.log('  multi - Multi-user test with room creation');
        console.log('  stress [numUsers] [messagesPerUser] - Stress test');
        console.log('  interactive - Interactive mode with full control');
    }
    
    tester.cleanup();
  } catch (error) {
    console.error('Test failed:', error);
    tester.cleanup();
    process.exit(1);
  }
}

if (require.main === module) {
  main();
}