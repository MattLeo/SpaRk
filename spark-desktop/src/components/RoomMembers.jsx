import React from 'react';
import './RoomMembers.css';

const RoomMembers = ({ members, currentRoom }) => {
    const getStatusColor = (presence) => {
        switch (presence) {
            case 'Online':
                return '#4ade80';
            case 'Away':
                return '#fbbf24';
            case 'DoNotDisturb':
                return '#ef4444';
            case 'AppearOffline':
            case 'Offline':
                return '#6b7280';
            default:
                return '#6b7280';
        }
    };

    const getStatusIcon = (presence) => {
        switch (presence) {
            case 'Online': return '🟢';
            case 'Away': return '🟡';
            case 'DoNotDisturb': return '⛔';
            default: return '⚫';
        }
    };

    const sortedMembers = [...members].sort((a,b) => {
        const statusOrder = {
            'Online': 0,
            'Away': 1,
            'DoNotDisturb': 2,
            'AppearOffline': 3,
            'Offline': 3,
        };
        const statusDiff = statusOrder[a.presence] - statusOrder[b.presence];
        if (statusDiff !== 0) return statusDiff;
        return a.username.localeCompare(b.username);
    });

    const onlineCount = members.filter(m => m.presence === 'Online').length;
    const awayCount = members.filter(m => m.presence === 'Away').length;
    const dndCount = members.filter(m => m.presence === 'DoNotDisturb').length;
    const offlineCount = members.filter(m => m.presence === 'Offline' || m.presence === 'AppearOffline').length;

    return (
        <div className='room-members-panel'>
            <div className='members-header'>
                <h3>Room Members</h3>
                <div className='member-count'>
                    {members.length} {members.length === 1 ? 'member' : 'members'}
                </div>
            </div>
            <div className='presence-summary'>
                {onlineCount > 0 && (
                    <div className='presence-stat'>
                        <span className='presence-dot' style={{ backgroundColor: '#4ade80'}}></span>
                        <span>{onlineCount} Online</span>
                    </div>
                )}
                {awayCount > 0 && (
                    <div className='presence-stat'>
                        <span className='presence-dot' style={{ backgroundColor: '#fbbf24' }}></span>
                        <span>{awayCount} Away</span>
                    </div>
                )}
                {dndCount > 0 && (
                    <div className='presence-stat'>
                        <span className='presence-dot' style={{ backgroundColor: '#ef4444' }}></span>
                        <span>{dndCount} Do Not Disturb</span>
                    </div>
                )}
                {offlineCount > 0 && (
                    <div className='presence-stat'>
                        <span className='presence-dot' style={{ backgroundColor: '#6b7280' }}></span>
                        <span>{offlineCount} Offline</span>
                    </div>
                )}
            </div>
            
            <div className='members-list'>
                {sortedMembers.length === 0 ? (
                    <div className='no-members'>No members in this room</div>
                ) : (
                    sortedMembers.map((member) => (
                        <div key={member.user_id} className='member-item'>
                            <div className='member-avator'>
                                <div className='avatar-circle'>
                                    {member.username.charAt(0).toUpperCase()}
                                </div>
                            </div>
                            <div className='member-info'>
                                <div className='member-username'>{member.username}</div>
                                <div className='member-status'>
                                    {member.status || `${getStatusIcon(member.presence)} ${member.presence}`}
                                </div>
                            </div>
                        </div>
                    ))
                )}
            </div>
        </div>
    );
};

export default RoomMembers;