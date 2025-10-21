import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import './Chat.css';
import editIcon from '../assets/edit-white.png';
import deleteIcon from '../assets/delete-white.png';

function Chat({ user, onLogout }) {
    const [rooms, setRooms] = useState([]);
    const [currentRoom, setCurrentRoom] = useState(null);
    const [messages, setMessages] = useState({});
    const [messageInput, setMessageInput] = useState('');
    const [connected, setConnected] = useState(false);
    const [error, setError] = useState('');
    const [availableRooms, setAvailableRooms] = useState([]);
    const [showAvailableRooms, setShowAvailableRooms] = useState(false);
    const [isUserScrolling, setIsUserScrolling] = useState(false);
    const messagesEndRef = useRef(null);
    const messagesContainerRef = useRef(null);
    const [editingMessageId, setEditingMessageId] = useState(null);
    const [editContent, setEditContent] = useState('');

    useEffect(() => {
        const token = localStorage.getItem('authToken');
        if (token) {
            connectWebSocket(token);
        }

        const unlistenMessage = listen('ws-message', (event) => {
            handleWebSocketMessage(event.payload);
        });

        const unlistenClosed = listen('ws-closed', (event) => {
            setConnected(false);
            setError('WebSocket connection closed');
        });

        const unlistenError = listen('ws-error', (event) => {
            setError(`Websocket Error: ${event.payload}`);
        });

        return () => {
            unlistenMessage.then(fn => fn());
            unlistenClosed.then(fn => fn());
            unlistenError.then(fn => fn());
        };
    }, []);

    const connectWebSocket = async (token) => {
        try {
            await invoke('connect_websocket', { token });
            setConnected(true);
            setError('');
        } catch (err) {
            setError(String(err));
        }
    };

    useEffect(() => {
        if (!isUserScrolling) {
            scrollToBottom();
        }
    }, [messages, currentRoom, isUserScrolling]);

    const handleWebSocketMessage = (msg) => {
        switch (msg.type) {
            case 'Authenticated':
                console.log('Authenticated:', msg);
                break;

            case 'RoomCreated':
                setRooms(prev => [...prev, {id: msg.room_id, name: msg.room_name }]);
                setCurrentRoom(msg.room_id);
                break;

            case 'RoomList':
                setAvailableRooms(msg.rooms);
                break;

            case 'RoomJoined':
                setRooms(prev => [...prev, {id: msg.room_id, name: msg.room_name }]);
                if (!currentRoom) {
                    setCurrentRoom(msg.room_id);
                }
                break;
            
            case 'RoomLeft':
                setRooms(prev => prev.filter(r => r.id !== msg.room_id));
                if (currentRoom === msg.room_id) {
                    if (rooms.length > 1) {
                        setCurrentRoom(rooms.at(-1));
                    } else {
                        setCurrentRoom(null);
                    }
                }
                break;

            case 'NewMessage':
                const roomId = msg.message.room_id;
                setMessages(prev => ({
                    ...prev,
                    [roomId]: [...(prev[roomId] || []), msg.message]
                }));
                break;
            
            case 'RoomHistory':
                setMessages(prev => ({
                    ...prev,
                    [msg.room_id]: msg.messages.reverse()
                }));
                break;

            case 'MessageEdited':
                setMessages(prev=> ({
                    ...prev,
                    [msg.room_id]: (prev[msg.room_id] || []).map(m => 
                        m.id === msg.message_id 
                        ? { ...m, content: msg.new_content, is_edited: true, edited_at: msg.edited_at}
                        : m
                    )
                }));

                if (editingMessageId === msg.message_id) {
                    setEditingMessageId(null);
                    setEditContent('');
                }
                break;

            case 'MessageDeleted':
                setMessages(prev=> ({
                    ...prev,
                    [msg.room_id]: (prev[msg.room_id] || []).filter(m => m.id !== msg.message_id)
                }));
                break;

            case 'UserJoined':
                console.log(`User ${msg.username} joined ${msg.room_id}`);
                break;

            case 'UserLeft':
                console.log(`User ${msg.username} left ${msg.room_id}`);
                break;

            case 'Error':
                setError(msg.message);
                break;

            default:
                console.log('Unknown message type received:', msg);
        }
    };

    const createRoom = async (name, desc) => {
        try {
            await invoke('ws_create_room', { name, desc });
        } catch (err) {
            setError(String(err));
        }
    };

    const getAllRooms = async () => {
        try {
            await invoke('ws_get_all_rooms');
            setShowAvailableRooms(true);
        } catch (err) {
            setError(String(err));
        }
    }

    const joinRoom = async (roomId) => {
        try {
            await invoke('ws_join_room', { roomId });
            await invoke('ws_get_room_history', { roomId, limit: 100, offset: 0 });
        } catch (err) {
            setError(String(err));
        }
    };

    const leaveRoom = async (roomId) => {
        try {
            await invoke('ws_leave_room', { roomId });
        } catch (err) {
            setError(String(err));
        }
    };

    const sendMessage = async (e) => {
        e.preventDefault();
        if (!messageInput.trim() || !currentRoom) return;

        try {
            await invoke('ws_send_message', {
                roomId: currentRoom,
                content: messageInput
            });
            setMessageInput('');
        } catch (err) {
            setError(String(err));
        }
    };

    const startEdit = (message) => {
        setEditingMessageId(message.id);
        setEditContent(message.content);
    };

    const cancelEdit = () => {
        setEditingMessageId(null);
        setEditContent('');
    }

    const saveEdit = async (messageId) => {
        if (!editContent.trim()) return;

        try {
            await invoke('ws_edit_message', {
                roomId: currentRoom,
                messageId: messageId,
                newContent: editContent
            });
            setEditingMessageId(null);
            setEditContent('');
        } catch (err) {
            setError(String(err));
        }
    }

    const deleteMessage = async (messageId) => {
        if (!window.confirm('Are you sure you want to delete this message?')) {
            return;
        }

        try {
            await invoke('ws_delete_message', {
                roomId: currentRoom,
                messageId: messageId,
            });
        } catch (err) {
            setError(String(err));
        }
    }

    const handleRoomSelect = (roomId) => {
        setCurrentRoom(roomId);
    };

    const isNearBottom = () => {
        if (!messagesContainerRef.current) return true;
        const { scrollTop, scrollHeight, clientHeight } = messagesContainerRef.current;
        return scrollHeight - scrollTop - clientHeight < 100;
    };

    const handleScroll = () => {
        const nearBottom = isNearBottom();
        setIsUserScrolling(!nearBottom);
    };

    const scrollToBottom = () => {
        messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
    }


    const currentMessages = currentRoom ? (messages[currentRoom] || []) : [];
    const currentRoomName = rooms.find(r => r.id === currentRoom)?.name || 'Select a room';

    return (
        <div className='chat-container'>
            <div className='chat-sidebar'>
                <div className='sidebar-header'>
                    <h2>Rooms</h2>
                    <button onClick={onLogout} className='logout-btn-small'>Logout</button>
                </div>

                <div className='room-list'>
                    {rooms.map(room => (
                        <div className={`room-item ${currentRoom === room.id ? 'active' : ''}`}
                            onClick={() => handleRoomSelect(room.id)}
                        >
                            <span className='room-name'>{room.name}</span>
                            <button
                                onClick={(e) => {
                                    e.stopPropagation();
                                    leaveRoom(room.id);
                                }}
                                className='leave-btn'
                        >×</button>
                        </div>
                    ))}
                </div>

                <div className="join-room-section">
                    <h3>Create Room</h3>
                    <input
                        type="text"
                        placeholder="Room name..."
                        id = 'room-name-input'
                    />
                    <input
                        type="text"
                        placeholder="Room description..."
                        id = 'room-desc-input'
                        style={{ marginTop: '8px' }}
                    />
                    <button onClick={() => {
                        const nameInput = document.getElementById('room-name-input');
                        const descInput = document.getElementById('room-desc-input');
                        const name = nameInput?.value.trim();
                        const desc = descInput?.value.trim() || 'No description';
                        if (name) {
                            createRoom(name, desc);
                            if (nameInput) nameInput.value = '';
                            if (descInput) descInput.value = '';
                        }
                    }} className='create-room-btn'>Create Room</button>
                    <button onClick={getAllRooms} className="browse-rooms-btn">
                        Browse Available Rooms
                    </button>

                    {showAvailableRooms && (
                        <div className="available-rooms-list">
                        <h4>Available Rooms</h4>
                        {availableRooms.map(room => (
                            <div key={room.id} className="available-room-item">
                            <div className="available-room-info">
                                <strong>{room.name}</strong>
                                <small>{room.desc}</small>
                            </div>
                            <button onClick={() => {
                                joinRoom(room.id);
                                setShowAvailableRooms(false);
                            }} className="join-available-btn">
                                Join
                            </button>
                            </div>
                        ))}
                        {availableRooms.length === 0 && (
                            <p className="no-rooms">No rooms available</p>
                        )}
                        <button onClick={() => setShowAvailableRooms(false)} className="close-list-btn">
                            Close
                        </button>
                        </div>
                    )}
                    </div>

                <div className='connection-status'>
                    <span className={connected ? 'connected': 'disconnected'}>
                        {connected ? '● Connected' : '○ Disconnected'}
                    </span>
                </div>
            </div>
            <div className='chat-main'>
                <div className='chat-header'>
                    <h2>{currentRoomName}</h2>
                    <span className='user-badge'>{user?.username}</span>
                </div>

                {error && (
                    <div className='error-banner'>
                        {error}
                        <button onClick={() => setError('')}>×</button>
                    </div>
                )}

                <div className='messages-container' ref={messagesContainerRef} onScroll={handleScroll}>
                    {currentMessages.map((msg, idx) => {
                        const isOwnMessage = msg.sender_username === user.username;
                        const isEditing = editingMessageId === msg.id;

                        return (
                            <div key={idx} className='message'>
                                <div className='message-header'>
                                    <span className='message-sender'>{msg.sender_username}</span>
                                    <div className='message-header-right'>
                                        <span className='message-time'>{new Date(msg.sent_at).toLocaleTimeString()}</span>
                                        {msg.is_edited && <span className='edited-indicator'>(edited at {new Date(msg.edited_at).toLocaleDateString()})</span>}
                                        {isOwnMessage && !isEditing && (
                                            <div className='message-actions'>
                                                <button className='edit-btn' onClick={() => startEdit(msg)}>
                                                    <img src={editIcon} alt='Edit' />
                                                </button>
                                                <button className='delete-btn' onClick={() => deleteMessage(msg.id)}>
                                                    <img src={deleteIcon} alt='Delete' />
                                                </button>
                                            </div>
                                        )}
                                    </div>
                                </div>
                                {isEditing ? (
                                    <div className='message-edit-form'>
                                        <input
                                            type='text'
                                            value={editContent}
                                            onChange={(e) => setEditContent(e.target.value)}
                                            className='message-edit-input'
                                            autoFocus
                                            onKeyDown={(e) => {
                                                if (e.key === 'Enter') saveEdit(msg.id);
                                                else if (e.key === 'Escape') cancelEdit();
                                            }}
                                        />
                                        <div className='message-edit-actions'>
                                            <button className='save-edit-btn' onClick={() => saveEdit(msg.id)}>Save</button>
                                            <button className='cancel-edit-btn' onClick={() => cancelEdit()}>Cancel</button>
                                        </div>
                                    </div>
                                ) : (
                                    <div className='message-content'>{msg.content}</div>
                                )}
                            </div>
                        );
                    })}
                    <div ref={messagesEndRef} />
                    {currentMessages.length === 0 && currentRoom && (
                        <div className='no-messages'>No messages yet. Get the conversation started!</div>
                    )}
                    {!currentRoom && (
                        <div className='no-messages'>Select a room to start chatting</div>
                    )}
                </div>

                {isUserScrolling && currentRoom && (
                    <div className='scroll-to-bottom'>
                        <button onClick={scrollToBottom} className='scroll-to-bottom-btn'>
                            ↓ New messages
                        </button>
                    </div>
                )}
                
                {currentRoom && (
                    <form onSubmit={sendMessage} className='message-input-form'>
                        <input
                            type='text'
                            value={messageInput}
                            onChange={(e) => setMessageInput(e.target.value)}
                            placeholder='Type a message...'
                            className='message-input'
                        />
                        <button type='submit' className='send-btn'>Send</button>
                    </form>
                )}
            </div>
        </div>
    );
}

export default Chat;